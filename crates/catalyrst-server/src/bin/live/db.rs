use super::*;

pub(crate) struct LiveDatabase {
    pub(crate) pool: PgPool,
    pub(crate) entity_cache: Arc<RwLock<EntityCache>>,
    pub(crate) profile_lru: Arc<Mutex<ProfileLru>>,
    pub(crate) prefix_ids_cache: Arc<Mutex<PrefixIdsCache>>,
}

const NON_CANONICAL_INTERN_CAP: usize = 64;

fn non_canonical_intern_pool() -> &'static dashmap::DashMap<String, &'static str> {
    use std::sync::OnceLock;
    static POOL: OnceLock<dashmap::DashMap<String, &'static str>> = OnceLock::new();
    POOL.get_or_init(dashmap::DashMap::new)
}

pub(crate) fn intern_entity_type(s: &str) -> &'static str {
    match s {
        "profile" => "profile",
        "scene" => "scene",
        "wearable" => "wearable",
        "emote" => "emote",
        "store" => "store",
        "outfits" => "outfits",
        _ => {
            let pool = non_canonical_intern_pool();
            if let Some(existing) = pool.get(s) {
                return *existing;
            }
            if pool.len() >= NON_CANONICAL_INTERN_CAP {
                return "unknown";
            }

            let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
            pool.insert(s.to_string(), leaked);
            leaked
        }
    }
}

const POINTER_CHANGES_SELECT: &str = r#"
            SELECT
                dep1.id AS deployment_id,
                dep1.entity_type,
                dep1.entity_id,
                dep1.entity_pointers,
                date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
                date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
                dep1.deployer_address,
                dep1.version,
                dep1.entity_metadata,
                dep1.auth_chain
            FROM deployments AS dep1
            "#;

#[async_trait]
impl Database for LiveDatabase {
    async fn active_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError> {
        if pointers.is_empty() {
            return Ok(vec![]);
        }

        let lower_pointers: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();

        let mut results: Vec<Value> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut uncached_pointers: Vec<String> = Vec::new();

        {
            let cache = self.entity_cache.read().await;
            for ptr in &lower_pointers {
                if let Some(entity_id) = cache.pointer_to_id.get(ptr) {
                    if seen_ids.insert(entity_id.clone()) {
                        if let Some(entity) = cache.by_id.get(entity_id) {
                            if let Ok(val) = serde_json::from_slice::<Value>(&entity.bytes) {
                                results.push(val);
                            }
                        }
                    }
                } else {
                    uncached_pointers.push(ptr.clone());
                }
            }
        }

        if !uncached_pointers.is_empty() {
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id,
                    dep.entity_type,
                    dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version,
                    dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.ctid)
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM active_pointers ap
                INNER JOIN deployments dep ON dep.entity_id = ap.entity_id
                WHERE ap.pointer = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(&uncached_pointers)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            for row in rows {
                if seen_ids.insert(row.entity_id.clone()) {
                    let entity_type = row.entity_type.clone();
                    let entity_id = row.entity_id.clone();
                    let entities = build_entities_from_rows(vec![row]);
                    if let Some(value) = entities.into_iter().next() {
                        if entity_type == "profile" {
                            let mut lru = self.profile_lru.lock().await;
                            lru.insert(entity_id, value.clone());
                        }
                        results.push(value);
                    }
                }
            }
        }

        Ok(results)
    }

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut results: Vec<Value> = Vec::new();
        let mut uncached_ids: Vec<String> = Vec::new();

        {
            let cache = self.entity_cache.read().await;
            let lru = self.profile_lru.lock().await;
            for id in ids {
                if let Some(entity) = cache.by_id.get(id) {
                    if let Ok(val) = serde_json::from_slice::<Value>(&entity.bytes) {
                        results.push(val);
                    }
                } else if let Some(value) = lru.get(id) {
                    results.push(value.clone());
                } else {
                    uncached_ids.push(id.clone());
                }
            }
        }

        if !uncached_ids.is_empty() {
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id,
                    dep.entity_type,
                    dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version,
                    dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.ctid)
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM deployments dep
                WHERE dep.entity_id = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(&uncached_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            for row in rows {
                let entity_type = row.entity_type.clone();
                let entity_id = row.entity_id.clone();
                let entities = build_entities_from_rows(vec![row]);
                if let Some(value) = entities.into_iter().next() {
                    if entity_type == "profile" {
                        let mut lru = self.profile_lru.lock().await;
                        lru.insert(entity_id, value.clone());
                    }
                    results.push(value);
                }
            }
        }

        Ok(results)
    }

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError> {
        let cached = {
            let cache = self.prefix_ids_cache.lock().await;
            cache.get(prefix)
        };

        let entity_ids: Arc<Vec<String>> = match cached {
            Some(ids) => ids,
            None => {
                let ids = catalyrst_db::pointers_repository::get_item_entities_ids_matching_collection_urn_prefix(
                    &self.pool, prefix,
                )
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                let ids = Arc::new(ids);
                let mut cache = self.prefix_ids_cache.lock().await;
                cache.insert(prefix.to_string(), ids.clone());
                ids
            }
        };

        let total = entity_ids.len() as i64;

        if entity_ids.is_empty() {
            return Ok(PrefixQueryResult {
                total: 0,
                entities: vec![],
            });
        }

        let start = offset as usize;
        let end = ((offset + limit) as usize).min(entity_ids.len());
        if start >= entity_ids.len() {
            return Ok(PrefixQueryResult {
                total,
                entities: vec![],
            });
        }
        let page_ids: Vec<String> = entity_ids[start..end].to_vec();

        let entities = self.active_entities_by_ids(&page_ids).await?;

        Ok(PrefixQueryResult { total, entities })
    }

    async fn active_entity_ids_by_content_hash(
        &self,
        hash: &str,
    ) -> Result<Vec<String>, DatabaseError> {
        catalyrst_db::deployments_repository::get_active_deployments_by_content_hash(
            &self.pool, hash,
        )
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn get_deployments(
        &self,
        options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError> {
        let offset = curate_offset(options.offset);
        let limit = curate_limit(options.limit);
        let fetch_limit = limit + 1;

        let needs_audit = options.fields.iter().any(|f| f == "auditInfo");
        let needs_content = options.fields.iter().any(|f| f == "content");

        let sorting_field = options
            .sorting_field
            .as_deref()
            .unwrap_or("local_timestamp");
        let sorting_order = options.sorting_order.as_deref().unwrap_or("DESC");

        let ts_col = match sorting_field {
            "entity_timestamp" => "entity_timestamp",
            _ => "local_timestamp",
        };
        let order = match sorting_order {
            "ASC" => "ASC",
            _ => "DESC",
        };

        let auth_select = if needs_audit {
            "dep1.auth_chain, dep1.deployer_address,"
        } else {
            "NULL::json AS auth_chain, dep1.deployer_address,"
        };

        let mut sql = format!(
            r#"
            SELECT
                dep1.id,
                dep1.entity_type,
                dep1.entity_id,
                dep1.entity_pointers,
                date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
                dep1.entity_metadata,
                {}
                dep1.version,
                date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
                dep1.deleter_deployment
            FROM deployments AS dep1
            "#,
            auth_select,
        );

        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx: usize = 1;

        let from_val = options.from.map(|f| f as f64);
        let to_val = options.to.map(|t| t as f64);
        let last_id = options.last_id.as_deref();

        if let Some(_from) = from_val {
            if ts_col == "local_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "ASC" {
                        conditions.push(format!(
                            "(dep1.local_timestamp, LOWER(dep1.entity_id)) > (to_timestamp(${ts} / 1000.0), LOWER(${next}))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.local_timestamp >= to_timestamp(${} / 1000.0)",
                            param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.local_timestamp >= to_timestamp(${} / 1000.0)",
                        param_idx
                    ));
                    param_idx += 1;
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "ASC" {
                        conditions.push(format!(
                            "(dep1.entity_timestamp, LOWER(dep1.entity_id)) > (to_timestamp(${ts} / 1000.0), LOWER(${next}))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.entity_timestamp >= to_timestamp(${} / 1000.0)",
                            param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.entity_timestamp >= to_timestamp(${} / 1000.0)",
                        param_idx
                    ));
                    param_idx += 1;
                }
            }
        }
        if let Some(_to) = to_val {
            if ts_col == "local_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "DESC" {
                        conditions.push(format!(
                            "(dep1.local_timestamp, LOWER(dep1.entity_id)) < (to_timestamp(${ts} / 1000.0), LOWER(${next}))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.local_timestamp <= to_timestamp(${} / 1000.0)",
                            param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.local_timestamp <= to_timestamp(${} / 1000.0)",
                        param_idx
                    ));
                    param_idx += 1;
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "DESC" {
                        conditions.push(format!(
                            "(dep1.entity_timestamp, LOWER(dep1.entity_id)) < (to_timestamp(${ts} / 1000.0), LOWER(${next}))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.entity_timestamp <= to_timestamp(${} / 1000.0)",
                            param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.entity_timestamp <= to_timestamp(${} / 1000.0)",
                        param_idx
                    ));
                    param_idx += 1;
                }
            }
        }

        if !options.entity_types.is_empty() {
            conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
            param_idx += 1;
        }
        if !options.entity_ids.is_empty() {
            conditions.push(format!("dep1.entity_id = ANY(${})", param_idx));
            param_idx += 1;
        }
        if options.only_currently_pointed == Some(true) {
            conditions.push("dep1.deleter_deployment IS NULL".into());
        }
        if !options.pointers.is_empty() {
            conditions.push(format!("dep1.entity_pointers && ${}", param_idx));
            param_idx += 1;
        }

        if !options.deployed_by.is_empty() {
            conditions.push(format!(
                "LOWER(dep1.deployer_address) = ANY(${})",
                param_idx
            ));
            param_idx += 1;
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " ORDER BY dep1.\"{}\" {}, LOWER(dep1.entity_id) {}",
            ts_col, order, order,
        ));
        sql.push_str(&format!(" LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));

        #[derive(sqlx::FromRow)]
        #[allow(dead_code)]
        struct DepRow {
            id: i32,
            entity_type: String,
            entity_id: String,
            entity_pointers: Vec<String>,
            entity_timestamp: f64,
            entity_metadata: Option<Value>,
            deployer_address: String,
            version: String,
            auth_chain: Option<Value>,
            local_timestamp: f64,
            #[allow(dead_code)]
            deleter_deployment: Option<i32>,
        }

        let mut query = sqlx::query_as::<_, DepRow>(sqlx::AssertSqlSafe(sql));

        if let Some(from) = from_val {
            if ts_col == "local_timestamp" {
                if let Some(lid) = last_id {
                    if order == "ASC" {
                        query = query.bind(lid.to_string()).bind(from);
                    } else {
                        query = query.bind(from);
                    }
                } else {
                    query = query.bind(from);
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(lid) = last_id {
                    if order == "ASC" {
                        query = query.bind(lid.to_string()).bind(from);
                    } else {
                        query = query.bind(from);
                    }
                } else {
                    query = query.bind(from);
                }
            }
        }
        if let Some(to) = to_val {
            if ts_col == "local_timestamp" {
                if let Some(lid) = last_id {
                    if order == "DESC" {
                        query = query.bind(lid.to_string()).bind(to);
                    } else {
                        query = query.bind(to);
                    }
                } else {
                    query = query.bind(to);
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(lid) = last_id {
                    if order == "DESC" {
                        query = query.bind(lid.to_string()).bind(to);
                    } else {
                        query = query.bind(to);
                    }
                } else {
                    query = query.bind(to);
                }
            }
        }
        if !options.entity_types.is_empty() {
            query = query.bind(options.entity_types.clone());
        }
        if !options.entity_ids.is_empty() {
            query = query.bind(options.entity_ids.clone());
        }
        if !options.pointers.is_empty() {
            let lower: Vec<String> = options.pointers.iter().map(|p| p.to_lowercase()).collect();
            query = query.bind(lower);
        }
        if !options.deployed_by.is_empty() {
            let lower: Vec<String> = options
                .deployed_by
                .iter()
                .map(|a| a.to_lowercase())
                .collect();
            query = query.bind(lower);
        }

        query = query.bind(fetch_limit).bind(offset);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let more_data = rows.len() as i64 > limit;
        let rows: Vec<DepRow> = if more_data {
            rows.into_iter().take(limit as usize).collect()
        } else {
            rows
        };

        let deployment_ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        let content_map = if !needs_content || deployment_ids.is_empty() {
            HashMap::new()
        } else {
            let cf_rows: Vec<(i32, String, String)> = sqlx::query_as(
                "SELECT deployment, content_hash, key FROM content_files WHERE deployment = ANY($1) ORDER BY ctid"
            )
            .bind(&deployment_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let mut map: HashMap<i32, Vec<(String, String)>> = HashMap::new();
            for (dep_id, hash, key) in cf_rows {
                map.entry(dep_id).or_default().push((key, hash));
            }
            map
        };

        let empty_auth = Value::Array(vec![]);
        let empty_content: Vec<(String, String)> = vec![];

        let deployment_values: Vec<Value> = rows
            .iter()
            .map(|d| {
                let content = content_map.get(&d.id).unwrap_or(&empty_content);
                let metadata = d.entity_metadata.as_ref().and_then(|m| m.get("v").cloned());

                let auth_chain_ref = d.auth_chain.as_ref().unwrap_or(&empty_auth);
                let interned_type = intern_entity_type(&d.entity_type);

                let content_arr: Vec<Value> = content
                    .iter()
                    .map(|(key, hash)| json!({"key": key, "hash": hash}))
                    .collect();

                let mut obj = json!({
                    "entityType": interned_type,
                    "entityId": &d.entity_id,
                    "entityTimestamp": d.entity_timestamp as i64,
                    "pointers": &d.entity_pointers,
                    "content": content_arr,
                    "deployedBy": &d.deployer_address,
                    "entityVersion": &d.version,
                    "auditInfo": {
                        "version": &d.version,
                        "authChain": auth_chain_ref,
                        "localTimestamp": d.local_timestamp as i64,
                    },
                    "localTimestamp": d.local_timestamp as i64,
                });

                if let Some(ref m) = metadata {
                    obj["metadata"] = m.clone();
                }

                obj
            })
            .collect();

        let filters_json = serde_json::to_value(&DeploymentFiltersResponse {
            pointers: &options.pointers,
            entity_types: &options.entity_types,
            entity_ids: &options.entity_ids,
            from: options.from,
            to: options.to,
            only_currently_pointed: options.only_currently_pointed,
            deployed_by: &options.deployed_by,
        })
        .unwrap_or_default();

        Ok(DeploymentQueryResult {
            deployments: deployment_values,
            filters: filters_json,
            pagination: PaginationResult {
                offset,
                limit,
                more_data,
                next: None,
                last_id: options.last_id.clone(),
            },
        })
    }

    async fn get_pointer_changes(
        &self,
        options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError> {
        let offset = curate_offset(options.offset);
        let limit = curate_limit(options.limit);
        let fetch_limit = limit + 1;

        let sorting_field = options
            .sorting_field
            .as_deref()
            .unwrap_or("local_timestamp");
        let sorting_order = options.sorting_order.as_deref().unwrap_or("DESC");

        let ts_col = match sorting_field {
            "entity_timestamp" => "entity_timestamp",
            _ => "local_timestamp",
        };
        let order = match sorting_order {
            "ASC" => "ASC",
            _ => "DESC",
        };

        let mut sql = String::from(POINTER_CHANGES_SELECT);

        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx: usize = 1;

        if let Some(_from) = options.from {
            conditions.push(format!(
                "dep1.{} >= to_timestamp(${} / 1000.0)",
                ts_col, param_idx
            ));
            param_idx += 1;
        }
        if let Some(_to) = options.to {
            if let Some(_lid) = options.last_id.as_deref() {
                if order == "DESC" {
                    conditions.push(format!(
                        "(dep1.{col}, LOWER(dep1.entity_id)) < (to_timestamp(${ts} / 1000.0), LOWER(${next}))",
                        next = param_idx, ts = param_idx + 1, col = ts_col,
                    ));
                    param_idx += 2;
                } else {
                    conditions.push(format!(
                        "dep1.{} <= to_timestamp(${} / 1000.0)",
                        ts_col, param_idx
                    ));
                    param_idx += 1;
                }
            } else {
                conditions.push(format!(
                    "dep1.{} <= to_timestamp(${} / 1000.0)",
                    ts_col, param_idx
                ));
                param_idx += 1;
            }
        }

        if !options.entity_types.is_empty() {
            conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
            param_idx += 1;
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " ORDER BY dep1.\"{}\" {}, LOWER(dep1.entity_id) {}",
            ts_col, order, order,
        ));

        sql.push_str(&format!(" LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));

        #[derive(sqlx::FromRow)]
        struct PointerChangeRow {
            deployment_id: i32,
            entity_type: String,
            entity_id: String,
            entity_pointers: Vec<String>,
            local_timestamp: f64,
            entity_timestamp: f64,
            deployer_address: String,
            version: String,
            entity_metadata: Option<Value>,
            auth_chain: Value,
        }

        let mut query = sqlx::query_as::<_, PointerChangeRow>(sqlx::AssertSqlSafe(sql));

        if let Some(from) = options.from {
            query = query.bind(from as f64);
        }
        if let Some(to) = options.to {
            if let Some(lid) = options.last_id.as_deref() {
                if order == "DESC" {
                    query = query.bind(lid.to_string()).bind(to as f64);
                } else {
                    query = query.bind(to as f64);
                }
            } else {
                query = query.bind(to as f64);
            }
        }
        if !options.entity_types.is_empty() {
            query = query.bind(&options.entity_types);
        }

        query = query.bind(fetch_limit).bind(offset);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let more_data = rows.len() as i64 > limit;
        let rows: Vec<PointerChangeRow> = if more_data {
            rows.into_iter().take(limit as usize).collect()
        } else {
            rows
        };

        let deltas: Vec<Value> = rows
            .iter()
            .map(|r| {
                let delta = PointerChangeDelta {
                    deployment_id: r.deployment_id as i64,
                    entity_type: intern_entity_type(&r.entity_type),
                    entity_id: &r.entity_id,
                    pointers: &r.entity_pointers,
                    entity_timestamp: r.entity_timestamp as i64,
                    metadata: r.entity_metadata.as_ref().and_then(|m| m.get("v")),
                    deployer_address: &r.deployer_address,
                    version: &r.version,
                    auth_chain: &r.auth_chain,
                    local_timestamp: r.local_timestamp as i64,
                };
                serde_json::to_value(&delta).unwrap_or_default()
            })
            .collect();

        let filters_json = serde_json::to_value(&PointerChangesFiltersResponse {
            entity_types: &options.entity_types,
            from: options.from,
            to: options.to,
            include_auth_chain: options.include_auth_chain,
        })
        .unwrap_or_default();

        Ok(PointerChangesQueryResult {
            deltas,
            filters: filters_json,
            pagination: PaginationResult {
                offset,
                limit,
                more_data,
                next: None,
                last_id: options.last_id.clone(),
            },
        })
    }

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
        #[derive(Serialize)]
        struct FailedDeploymentResponse {
            #[serde(rename = "entityId")]
            entity_id: String,
            #[serde(rename = "entityType")]
            entity_type: String,
            #[serde(rename = "failureTimestamp")]
            failure_timestamp: i64,
            reason: String,
            #[serde(rename = "authChain")]
            auth_chain: Value,
            #[serde(rename = "errorDescription")]
            error_description: String,
            #[serde(rename = "snapshotHash")]
            snapshot_hash: String,
        }

        let rows = catalyrst_db::failed_deployments_repository::get_snapshot_failed_deployments(
            &self.pool,
        )
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|fd| {
                serde_json::to_value(&FailedDeploymentResponse {
                    entity_id: fd.entity_id,
                    entity_type: fd.entity_type,
                    failure_timestamp: fd.failure_timestamp as i64,
                    reason: fd.reason,
                    auth_chain: fd.auth_chain,
                    error_description: fd.error_description,
                    snapshot_hash: fd.snapshot_hash,
                })
                .unwrap_or_default()
            })
            .collect())
    }

    async fn get_audit_info(
        &self,
        _entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError> {
        #[derive(sqlx::FromRow)]
        struct AuditRow {
            version: String,
            auth_chain: Value,
            local_timestamp: f64,
        }

        let row: Option<AuditRow> = sqlx::query_as(
            r#"
            SELECT
                version,
                auth_chain,
                date_part('epoch', local_timestamp) * 1000 AS local_timestamp
            FROM deployments
            WHERE entity_id = $1
            LIMIT 1
            "#,
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        #[derive(Serialize)]
        struct AuditInfoDetail {
            version: String,
            #[serde(rename = "authChain")]
            auth_chain: Value,
            #[serde(rename = "localTimestamp")]
            local_timestamp: i64,
        }

        Ok(row.map(|r| {
            serde_json::to_value(&AuditInfoDetail {
                version: r.version,
                auth_chain: r.auth_chain,
                local_timestamp: r.local_timestamp as i64,
            })
            .unwrap_or_default()
        }))
    }

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError> {
        let lower = pointer.to_lowercase();
        let pointers = vec![lower];
        let mut entities = self.active_entities_by_pointers(&pointers).await?;
        Ok(entities.pop())
    }

    async fn clear_failed_deployment(&self, entity_id: &str) -> Result<u64, DatabaseError> {
        let res = sqlx::query("DELETE FROM failed_deployments WHERE entity_id = $1")
            .bind(entity_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn clear_all_failed_deployments(&self) -> Result<u64, DatabaseError> {
        let res = sqlx::query("DELETE FROM failed_deployments")
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(res.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::POINTER_CHANGES_SELECT;

    #[test]
    fn pointer_changes_selects_entity_metadata() {
        assert!(POINTER_CHANGES_SELECT.contains("dep1.entity_metadata"));
        assert!(!POINTER_CHANGES_SELECT.contains("NULL::json"));
    }
}
