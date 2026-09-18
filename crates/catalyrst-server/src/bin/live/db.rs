use super::*;

pub(crate) struct LiveDatabase {
    pub(crate) pool: PgPool,
    pub(crate) deployment_schema: catalyrst_server::schema_migrations::DeploymentSchema,
    pub(crate) entity_cache: Arc<RwLock<EntityCache>>,
    pub(crate) profile_lru: Arc<Mutex<ProfileLru>>,
    pub(crate) prefix_ids_cache: Arc<Mutex<PrefixIdsCache>>,
}

const NON_CANONICAL_INTERN_CAP: usize = 64;

use catalyrst_db::failed_deployments_repository::SnapshotFailedDeployment;

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
    #[serde(rename = "retryCount")]
    retry_count: i32,
    #[serde(rename = "nextRetryAt")]
    next_retry_at: i64,
}

fn failed_deployment_value(fd: SnapshotFailedDeployment) -> Value {
    serde_json::to_value(&FailedDeploymentResponse {
        entity_id: fd.entity_id,
        entity_type: fd.entity_type,
        failure_timestamp: fd.failure_timestamp as i64,
        reason: fd.reason,
        auth_chain: fd.auth_chain,
        error_description: fd.error_description,
        snapshot_hash: fd.snapshot_hash,
        retry_count: fd.retry_count,
        next_retry_at: fd.next_retry_at as i64,
    })
    .unwrap_or_default()
}

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
                dep1.auth_chain
            FROM deployments AS dep1
            "#;

fn cached_doc(entity: &CachedEntity) -> EntityDoc {
    EntityDoc {
        id: entity.entity_id.clone(),
        json: entity.bytes.clone(),
    }
}

fn row_to_doc(row: ActiveEntityRow) -> EntityDoc {
    let id = row.entity_id.clone();
    let value = build_entities_from_rows(vec![row])
        .into_iter()
        .next()
        .unwrap_or(Value::Null);
    EntityDoc {
        id,
        json: Bytes::from(serde_json::to_vec(&value).unwrap_or_default()),
    }
}

fn docs_to_values(docs: Vec<EntityDoc>) -> Vec<Value> {
    docs.iter().filter_map(EntityDoc::to_value).collect()
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

impl LiveDatabase {
    /// Fills the profile LRU with what SQL returned unless a deployment landed while the read
    /// guard was released, in which case the rows may predate it and are not remembered.
    pub(crate) async fn remember_profiles(&self, generation: u64, profiles: Vec<EntityDoc>) {
        if profiles.is_empty() {
            return;
        }
        let cache = self.entity_cache.read().await;
        if cache.generation != generation {
            return;
        }
        let mut lru = self.profile_lru.lock().await;
        for doc in profiles {
            lru.insert(doc.id, doc.json);
        }
    }

    async fn prefix_ids_and_page(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<String>, Vec<EntityDoc>), DatabaseError> {
        let pattern = format!("{}%", escape_like(prefix));
        use sqlx::{FromRow, Row};
        let rows = sqlx::query(
            r#"
            WITH ids AS (
                SELECT p.entity_id, row_number() OVER () AS ord
                FROM active_pointers AS p WHERE p.pointer LIKE $1 ESCAPE '\'
            ),
            page AS (
                SELECT dep.entity_id, dep.entity_type, dep.entity_pointers, dep.entity_metadata,
                       date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                       dep.version, dep.id, ids.ord,
                       COALESCE(
                           (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key)
                            FROM content_files cf WHERE cf.deployment = dep.id),
                           '[]'::json
                       ) AS content_json
                FROM ids
                INNER JOIN deployments dep ON dep.entity_id = ids.entity_id
                WHERE ids.ord > $2 AND ids.ord <= $2 + $3
                  AND dep.deleter_deployment IS NULL
            )
            SELECT CASE WHEN row_number() OVER () = 1
                        THEN COALESCE((SELECT array_agg(entity_id ORDER BY ord) FROM ids), '{}')
                   END AS all_ids,
                   page.entity_id, page.entity_type, page.entity_pointers, page.entity_metadata,
                   page.entity_timestamp, page.version, page.id, page.content_json
            FROM (SELECT 1) marker LEFT JOIN page ON true
            ORDER BY page.ord
            "#,
        )
        .bind(&pattern)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut ids = Vec::new();
        let mut page = Vec::new();
        for row in rows {
            let decoded = (|| -> Result<(), sqlx::Error> {
                if let Some(all) = row.try_get::<Option<Vec<String>>, _>("all_ids")? {
                    ids = all;
                }
                if row.try_get::<Option<String>, _>("entity_id")?.is_some() {
                    page.push(row_to_doc(ActiveEntityRow::from_row(&row)?));
                }
                Ok(())
            })();
            decoded.map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        }
        Ok((ids, page))
    }
}

#[async_trait]
impl Database for LiveDatabase {
    async fn deployment_committed(&self, entity_id: &str) -> Result<(), DatabaseError> {
        if let Err(error) = invalidate_deployment_caches(
            &self.pool,
            &self.entity_cache,
            &self.profile_lru,
            &self.prefix_ids_cache,
            entity_id,
            self.deployment_schema.local_entities,
        )
        .await
        {
            // Invalidation evicts before refilling and clears every cache if it
            // cannot identify the affected entries. Reads safely fall back to SQL.
            tracing::warn!(entity_id, %error, "Deployment committed; cache refresh failed after eviction");
        }
        Ok(())
    }

    async fn active_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError> {
        Ok(docs_to_values(
            self.active_entity_docs_by_pointers(pointers).await?,
        ))
    }

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError> {
        Ok(docs_to_values(self.active_entity_docs_by_ids(ids).await?))
    }

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError> {
        let result = self
            .active_entity_docs_by_prefix(prefix, offset, limit)
            .await?;
        Ok(PrefixQueryResult {
            total: result.total,
            entities: docs_to_values(result.entities),
        })
    }

    async fn active_entity_docs_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<EntityDoc>, DatabaseError> {
        if pointers.is_empty() {
            return Ok(vec![]);
        }

        let lower_pointers: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();

        let mut results: Vec<EntityDoc> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut uncached_pointers: Vec<String> = Vec::new();

        let generation = {
            let cache = self.entity_cache.read().await;
            for ptr in &lower_pointers {
                if let Some(entity_id) = cache.pointer_to_id.get(ptr) {
                    if seen_ids.insert(entity_id.clone()) {
                        if let Some(entity) = cache.by_id.get(entity_id) {
                            results.push(cached_doc(entity));
                        }
                    }
                } else {
                    uncached_pointers.push(ptr.clone());
                }
            }
            cache.generation
        };

        if uncached_pointers.is_empty() {
            return Ok(results);
        }

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
                    (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key)
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

        let mut fresh_profiles = Vec::new();
        for row in rows {
            if seen_ids.insert(row.entity_id.clone()) {
                let is_profile = row.entity_type == "profile";
                let doc = row_to_doc(row);
                if is_profile {
                    fresh_profiles.push(doc.clone());
                }
                results.push(doc);
            }
        }
        self.remember_profiles(generation, fresh_profiles).await;

        Ok(results)
    }

    async fn active_entity_docs_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<EntityDoc>, DatabaseError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut results: Vec<EntityDoc> = Vec::new();
        let mut uncached_ids: Vec<String> = Vec::new();

        let generation = {
            let cache = self.entity_cache.read().await;
            let lru = self.profile_lru.lock().await;
            for id in ids {
                if let Some(entity) = cache.by_id.get(id) {
                    results.push(cached_doc(entity));
                } else if let Some(json) = lru.get(id) {
                    results.push(EntityDoc {
                        id: id.clone(),
                        json: json.clone(),
                    });
                } else {
                    uncached_ids.push(id.clone());
                }
            }
            cache.generation
        };

        if uncached_ids.is_empty() {
            return Ok(results);
        }

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
                    (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key)
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

        let mut fresh_profiles = Vec::new();
        for row in rows {
            let is_profile = row.entity_type == "profile";
            let doc = row_to_doc(row);
            if is_profile {
                fresh_profiles.push(doc.clone());
            }
            results.push(doc);
        }
        self.remember_profiles(generation, fresh_profiles).await;

        Ok(results)
    }

    async fn active_entity_docs_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixDocsResult, DatabaseError> {
        let cached = {
            let generation = self.entity_cache.read().await.generation;
            let ids = self.prefix_ids_cache.lock().await.get(prefix);
            (generation, ids)
        };

        let entity_ids: Arc<Vec<String>> = match cached {
            (_, Some(ids)) => ids,
            (generation, None) => {
                // Cold: the id list (memoized for later pages) and this page's entities in one statement.
                let (ids, page) = self.prefix_ids_and_page(prefix, offset, limit).await?;
                let ids = Arc::new(ids);
                {
                    let cache = self.entity_cache.read().await;
                    if cache.generation == generation {
                        self.prefix_ids_cache
                            .lock()
                            .await
                            .insert(prefix.to_string(), ids.clone());
                    }
                }
                return Ok(PrefixDocsResult {
                    total: ids.len() as i64,
                    entities: page,
                });
            }
        };

        let total = entity_ids.len() as i64;
        let start = offset as usize;
        let end = ((offset + limit) as usize).min(entity_ids.len());
        if start >= entity_ids.len() {
            return Ok(PrefixDocsResult {
                total,
                entities: vec![],
            });
        }
        let page_ids: Vec<String> = entity_ids[start..end].to_vec();

        let entities = self.active_entity_docs_by_ids(&page_ids).await?;

        Ok(PrefixDocsResult { total, entities })
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
        let content_select = if needs_content {
            "COALESCE((SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key) \
              FROM content_files cf WHERE cf.deployment = dep1.id), '[]'::json) AS content_json"
        } else {
            "'[]'::json AS content_json"
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
                dep1.deleter_deployment,
                {}
            FROM deployments AS dep1
            "#,
            auth_select, content_select,
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
            content_json: Value,
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

        let deployments: Vec<ControllerDeployment> = rows
            .iter()
            .map(|d| {
                let content = parse_content_json(&d.content_json);
                let metadata = d.entity_metadata.as_ref().and_then(|m| m.get("v").cloned());
                let auth_chain = d.auth_chain.clone().unwrap_or_else(|| Value::Array(vec![]));
                let interned_type = intern_entity_type(&d.entity_type).to_string();

                let content_items: Vec<DeploymentContent> = content
                    .iter()
                    .map(|(key, hash)| DeploymentContent {
                        key: key.clone(),
                        hash: hash.clone(),
                    })
                    .collect();

                ControllerDeployment {
                    entity_version: d.version.clone(),
                    entity_type: interned_type,
                    entity_id: d.entity_id.clone(),
                    entity_timestamp: d.entity_timestamp as i64,
                    deployed_by: d.deployer_address.clone(),
                    pointers: Some(d.entity_pointers.clone()),
                    content: Some(content_items),
                    metadata,
                    audit_info: Some(AuditInfo {
                        version: d.version.clone(),
                        auth_chain,
                        local_timestamp: d.local_timestamp as i64,
                    }),
                    local_timestamp: d.local_timestamp as i64,
                }
            })
            .collect();

        let filters = DeploymentsFilters {
            pointers: options.pointers.clone(),
            entity_types: options.entity_types.clone(),
            entity_ids: options.entity_ids.clone(),
            from: options.from,
            to: options.to,
            only_currently_pointed: options.only_currently_pointed,
            deployed_by: options.deployed_by.clone(),
        };

        Ok(DeploymentQueryResult {
            deployments,
            filters,
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

        let deltas: Vec<PointerChangeDelta> = rows
            .iter()
            .map(|r| PointerChangeDelta {
                deployment_id: r.deployment_id as i64,
                entity_type: intern_entity_type(&r.entity_type).to_string(),
                entity_id: r.entity_id.clone(),
                pointers: r.entity_pointers.clone(),
                entity_timestamp: r.entity_timestamp as i64,
                deployer_address: r.deployer_address.clone(),
                version: r.version.clone(),
                auth_chain: r.auth_chain.clone(),
                local_timestamp: r.local_timestamp as i64,
            })
            .collect();

        let filters = PointerChangesFilters {
            entity_types: options.entity_types.clone(),
            from: options.from,
            to: options.to,
            include_auth_chain: options.include_auth_chain,
        };

        Ok(PointerChangesQueryResult {
            deltas,
            filters,
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
        let rows = catalyrst_db::failed_deployments_repository::get_snapshot_failed_deployments(
            &self.pool,
        )
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(rows.into_iter().map(failed_deployment_value).collect())
    }

    async fn get_failed_deployments_page(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Value>, DatabaseError> {
        let rows = sqlx::query_as::<_, SnapshotFailedDeployment>(
            r#"
            SELECT
                entity_id AS "entityId",
                entity_type AS "entityType",
                date_part('epoch', failure_time) * 1000 AS "failureTimestamp",
                reason,
                auth_chain AS "authChain",
                error_description AS "errorDescription",
                snapshot_hash AS "snapshotHash",
                retry_count AS "retryCount",
                date_part('epoch', next_retry_at) * 1000 AS "nextRetryAt"
            FROM failed_deployments
            OFFSET $1 LIMIT $2
            "#,
        )
        .bind(offset.max(0))
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(rows.into_iter().map(failed_deployment_value).collect())
    }

    fn deployment_schema(&self) -> Option<catalyrst_server::schema_migrations::DeploymentSchema> {
        Some(self.deployment_schema)
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
            signer: Option<String>,
            origin: Option<String>,
            published_at: Option<f64>,
            tombstoned_at: Option<f64>,
            superseded: bool,
        }

        let provenance = if self.deployment_schema.local_entities {
            "le.signer, le.origin,
                    date_part('epoch', le.published_at) * 1000 AS published_at,
                    date_part('epoch', le.tombstoned_at) * 1000 AS tombstoned_at,
                    d.deleter_deployment IS NOT NULL AS superseded
             FROM deployments d
             LEFT JOIN local_entities le ON le.entity_id = d.entity_id"
        } else {
            "NULL::text AS signer, NULL::text AS origin,
                    NULL::float8 AS published_at, NULL::float8 AS tombstoned_at,
                    false AS superseded
             FROM deployments d"
        };
        let sql = format!(
            "SELECT d.version, d.auth_chain,
                    date_part('epoch', d.local_timestamp) * 1000 AS local_timestamp,
                    {provenance}
             WHERE d.entity_id = $1
             LIMIT 1"
        );
        let row: Option<AuditRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
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
            let mut value = serde_json::to_value(&AuditInfoDetail {
                version: r.version,
                auth_chain: r.auth_chain,
                local_timestamp: r.local_timestamp as i64,
            })
            .unwrap_or_default();
            if let (Some(signer), Some(origin), Some(published_at)) =
                (r.signer, r.origin, r.published_at)
            {
                value["localProvenance"] = catalyrst_server::land_publish::provenance_json(
                    signer,
                    origin,
                    published_at,
                    r.tombstoned_at,
                    r.superseded,
                );
            }
            value
        }))
    }

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError> {
        let lower = pointer.to_lowercase();
        let pointers = vec![lower];
        let mut entities = self.active_entities_by_pointers(&pointers).await?;
        Ok(entities.pop())
    }

    async fn clear_failed_deployment(&self, entity_id: &str) -> Result<u64, DatabaseError> {
        let res = sqlx::query!(
            "DELETE FROM failed_deployments WHERE entity_id = $1",
            entity_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn clear_all_failed_deployments(&self) -> Result<u64, DatabaseError> {
        let res = sqlx::query!("DELETE FROM failed_deployments")
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
    fn pointer_changes_does_not_select_entity_metadata() {
        assert!(!POINTER_CHANGES_SELECT.contains("entity_metadata"));
        assert!(POINTER_CHANGES_SELECT.contains("dep1.auth_chain"));
    }
}
