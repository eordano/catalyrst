use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::Serialize;
use serde_json::{json, Value};
use std::hint::black_box;

fn bench_hashing(c: &mut Criterion) {
    let data_1kb = vec![0xABu8; 1024];
    let data_64kb = vec![0xCDu8; 65_536];
    let data_1mb = vec![0xEFu8; 1_048_576];

    let mut group = c.benchmark_group("hashing/hash_bytes_v0");
    for (label, data) in [("1KB", &data_1kb), ("64KB", &data_64kb), ("1MB", &data_1mb)] {
        group.bench_with_input(BenchmarkId::from_parameter(label), data, |b, d| {
            b.iter(|| catalyrst_hashing::hash_bytes(black_box(d)))
        });
    }
    group.finish();

    let mut group = c.benchmark_group("hashing/hash_bytes_v1");
    for (label, data) in [("1KB", &data_1kb), ("64KB", &data_64kb), ("1MB", &data_1mb)] {
        group.bench_with_input(BenchmarkId::from_parameter(label), data, |b, d| {
            b.iter(|| catalyrst_hashing::hash_bytes_v1(black_box(d)))
        });
    }
    group.finish();
}

#[derive(Serialize)]
struct ContentEntry<'a> {
    key: &'a str,
    hash: &'a str,
}

#[derive(Serialize)]
struct AuditInfoResponse<'a> {
    version: &'a str,
    #[serde(rename = "authChain")]
    auth_chain: &'a Value,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(rename = "overwrittenBy")]
    overwritten_by: &'a Option<String>,
}

#[derive(Serialize)]
struct DeploymentItem<'a> {
    #[serde(rename = "entityType")]
    entity_type: &'a str,
    #[serde(rename = "entityId")]
    entity_id: &'a str,
    #[serde(rename = "entityTimestamp")]
    entity_timestamp: i64,
    pointers: &'a Vec<String>,
    content: Vec<ContentEntry<'a>>,
    #[serde(rename = "deployedBy")]
    deployed_by: &'a str,
    #[serde(rename = "entityVersion")]
    entity_version: &'a str,
    #[serde(rename = "auditInfo")]
    audit_info: AuditInfoResponse<'a>,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

fn bench_json_serialization(c: &mut Criterion) {
    let entity_type = "profile".to_string();
    let entity_id = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa".to_string();
    let pointers = vec!["0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_string()];
    let content_pairs = [
        (
            "body.png".to_string(),
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG".to_string(),
        ),
        (
            "face256.png".to_string(),
            "QmaozNR7DZHQK1ZcU9p7QdrshMvXqWK6gpu5rmrkPdT3L4".to_string(),
        ),
    ];
    let deployed_by = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_string();
    let version = "v3".to_string();
    let auth_chain = json!([
        {"type": "SIGNER", "payload": "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"}
    ]);
    let overwritten_by: Option<String> = None;
    let metadata = json!({"avatars": [{"name": "test", "description": "test avatar"}]});

    let build_item = || {
        let content: Vec<ContentEntry> = content_pairs
            .iter()
            .map(|(k, h)| ContentEntry {
                key: k.as_str(),
                hash: h.as_str(),
            })
            .collect();
        DeploymentItem {
            entity_type: &entity_type,
            entity_id: &entity_id,
            entity_timestamp: 1716000000000,
            pointers: &pointers,
            content,
            deployed_by: &deployed_by,
            entity_version: &version,
            audit_info: AuditInfoResponse {
                version: &version,
                auth_chain: &auth_chain,
                local_timestamp: 1716000001000,
                overwritten_by: &overwritten_by,
            },
            local_timestamp: 1716000001000,
            metadata: Some(&metadata),
        }
    };

    let mut group = c.benchmark_group("json_serialization");

    group.bench_function("typed_struct_to_vec", |b| {
        b.iter(|| {
            let item = build_item();
            black_box(serde_json::to_vec(&item).unwrap())
        })
    });

    group.bench_function("json_macro_to_vec", |b| {
        b.iter(|| {
            let content_val: Vec<Value> = content_pairs
                .iter()
                .map(|(k, h)| json!({"key": k, "hash": h}))
                .collect();

            let val = json!({
                "entityType": entity_type,
                "entityId": entity_id,
                "entityTimestamp": 1716000000000i64,
                "pointers": pointers,
                "content": content_val,
                "deployedBy": deployed_by,
                "entityVersion": version,
                "auditInfo": {
                    "version": version,
                    "authChain": auth_chain,
                    "localTimestamp": 1716000001000i64,
                    "overwrittenBy": serde_json::Value::Null,
                },
                "localTimestamp": 1716000001000i64,
                "metadata": metadata,
            });

            black_box(serde_json::to_vec(&val).unwrap())
        })
    });

    group.bench_function("double_serialization", |b| {
        b.iter(|| {
            let item = build_item();
            let val = serde_json::to_value(&item).unwrap();
            black_box(serde_json::to_vec(&val).unwrap())
        })
    });

    group.finish();
}

fn bench_auth_chain_verification(c: &mut Criterion) {
    use alloy::signers::{local::PrivateKeySigner, Signer};
    use catalyrst_crypto::auth_chain::{AuthLink, AuthLinkType};
    use catalyrst_crypto::verify::verify_auth_chain;

    let rt = tokio::runtime::Runtime::new().unwrap();

    let (chain, entity_payload) = rt.block_on(async {
        let root_key: PrivateKeySigner =
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .unwrap();
        let root_address = format!("{:#x}", root_key.address());

        let ephemeral_key: PrivateKeySigner =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let ephemeral_address = format!("{:#x}", ephemeral_key.address());

        let ephemeral_payload = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
            ephemeral_address
        );

        let ephemeral_sig = root_key
            .sign_message(ephemeral_payload.as_bytes())
            .await
            .unwrap();
        let ephemeral_sig_hex = ephemeral_sig.to_string();

        let entity_payload =
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string();

        let entity_sig = ephemeral_key
            .sign_message(entity_payload.as_bytes())
            .await
            .unwrap();
        let entity_sig_hex = entity_sig.to_string();

        let chain = vec![
            AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: root_address,
                signature: None,
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaEphemeral,
                payload: ephemeral_payload,
                signature: Some(ephemeral_sig_hex),
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaSignedEntity,
                payload: entity_payload.clone(),
                signature: Some(entity_sig_hex),
            },
        ];

        (chain, entity_payload)
    });

    verify_auth_chain(&chain, &entity_payload, Some(0)).expect("pre-built chain should verify");

    c.bench_function("auth_chain/verify_3_link_ecdsa", |b| {
        b.iter(|| {
            verify_auth_chain(black_box(&chain), black_box(&entity_payload), Some(0)).unwrap()
        })
    });
}

fn bench_hex_prefix(c: &mut Criterion) {
    let cids: Vec<String> = (0..1000)
        .map(|i| {
            format!(
                "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy{:04}",
                i
            )
        })
        .collect();

    c.bench_function("storage/hex_prefix_1000_cids", |b| {
        b.iter(|| {
            for cid in &cids {
                black_box(catalyrst_storage::hex_prefix(black_box(cid)));
            }
        })
    });
}

fn is_valid_content_hash(hash: &str) -> bool {
    if hash.starts_with("Qm")
        && hash.len() == 46
        && hash[2..].chars().all(|c| c.is_ascii_alphanumeric())
    {
        return true;
    }

    if hash.starts_with("ba")
        && hash.len() >= 52
        && hash
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return true;
    }

    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }

    false
}

fn bench_content_hash_validation(c: &mut Criterion) {
    let valid_cidv0 = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
    let valid_cidv1 = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenora7777";
    let valid_hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let invalid = "../../../etc/passwd";

    let mut group = c.benchmark_group("content_hash_validation");

    group.bench_function("valid_cidv0", |b| {
        b.iter(|| is_valid_content_hash(black_box(valid_cidv0)))
    });
    group.bench_function("valid_cidv1", |b| {
        b.iter(|| is_valid_content_hash(black_box(valid_cidv1)))
    });
    group.bench_function("valid_hex_sha256", |b| {
        b.iter(|| is_valid_content_hash(black_box(valid_hex)))
    });
    group.bench_function("invalid_path_traversal", |b| {
        b.iter(|| is_valid_content_hash(black_box(invalid)))
    });

    let hashes: Vec<&str> = (0..1000)
        .map(|i| match i % 4 {
            0 => valid_cidv0,
            1 => valid_cidv1,
            2 => valid_hex,
            _ => invalid,
        })
        .collect();

    group.bench_function("batch_1000_mixed", |b| {
        b.iter(|| {
            for h in &hashes {
                black_box(is_valid_content_hash(black_box(h)));
            }
        })
    });

    group.finish();
}

mod http_handlers {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use bytes::Bytes;
    use criterion::Criterion;
    use serde_json::{json, Value};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::hint::black_box;
    use tokio_util::io::ReaderStream;
    use tower::ServiceExt;

    use catalyrst_server::routes::build_router;
    use catalyrst_server::state::*;

    struct LiveContentStorage {
        inner: catalyrst_storage::ContentStorage,
    }

    #[async_trait]
    impl ContentStorage for LiveContentStorage {
        async fn retrieve(&self, hash: &str) -> Option<Bytes> {
            self.inner.retrieve(hash).await.ok().flatten()
        }

        async fn retrieve_stream(&self, hash: &str) -> Option<(Body, u64)> {
            let (path, _is_gzip) = self.inner.file_path(hash).await.ok()??;
            let file = tokio::fs::File::open(&path).await.ok()?;
            let metadata = file.metadata().await.ok()?;
            let size = metadata.len();
            let stream = ReaderStream::new(file);
            let body = Body::from_stream(stream);
            Some((body, size))
        }

        async fn retrieve_range(&self, hash: &str, start: u64, end: u64) -> Option<Bytes> {
            let data = self
                .inner
                .retrieve_uncompressed(hash)
                .await
                .ok()
                .flatten()?;
            let s = start as usize;
            let e = (end as usize).min(data.len().saturating_sub(1));
            if s > e || s >= data.len() {
                return None;
            }
            Some(data.slice(s..=e))
        }

        async fn file_info(&self, hash: &str) -> Option<FileInfo> {
            let info = self.inner.file_info(hash).await.ok()??;
            Some(FileInfo {
                size: Some(info.size),
                content_size: info.content_size,
                encoding: info.encoding,
            })
        }

        async fn exist_multiple(&self, hashes: &[String]) -> HashMap<String, bool> {
            let refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
            match self.inner.exist_multiple(&refs).await {
                Ok(results) => results.into_iter().collect(),
                Err(_) => hashes.iter().map(|h| (h.clone(), false)).collect(),
            }
        }
    }

    #[derive(Debug, sqlx::FromRow)]
    struct ActiveEntityRow {
        entity_id: String,
        entity_type: String,
        entity_pointers: Vec<String>,
        entity_metadata: Option<Value>,
        entity_timestamp: f64,
        version: String,
        #[allow(dead_code)]
        id: i32,
        content_json: Value,
    }

    fn parse_content_json(v: &Value) -> Vec<(String, String)> {
        match v.as_array() {
            Some(arr) => arr
                .iter()
                .filter_map(|entry| {
                    let key = entry.get("key")?.as_str()?;
                    let hash = entry.get("hash")?.as_str()?;
                    Some((key.to_string(), hash.to_string()))
                })
                .collect(),
            None => Vec::new(),
        }
    }

    fn build_entity_json(row: ActiveEntityRow) -> Value {
        let content = parse_content_json(&row.content_json);
        let metadata = row
            .entity_metadata
            .as_ref()
            .and_then(|m| m.get("v").cloned());
        let content_arr: Vec<Value> = content
            .iter()
            .map(|(k, h)| json!({"key": k, "hash": h}))
            .collect();

        json!({
            "version": row.version,
            "id": row.entity_id,
            "type": row.entity_type,
            "timestamp": row.entity_timestamp,
            "pointers": row.entity_pointers,
            "content": content_arr,
            "metadata": metadata,
        })
    }

    struct LiveDatabase {
        pool: PgPool,
    }

    #[async_trait]
    impl Database for LiveDatabase {
        async fn active_entities_by_pointers(
            &self,
            pointers: &[String],
        ) -> Result<Vec<Value>, DatabaseError> {
            if pointers.is_empty() {
                return Ok(vec![]);
            }
            let lower: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id, dep.entity_type, dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version, dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM active_pointers ap
                INNER JOIN deployments dep ON dep.entity_id = ap.entity_id
                WHERE ap.pointer = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(&lower)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let mut seen = std::collections::HashSet::new();
            Ok(rows
                .into_iter()
                .filter(|r| seen.insert(r.entity_id.clone()))
                .map(build_entity_json)
                .collect())
        }

        async fn active_entities_by_ids(
            &self,
            ids: &[String],
        ) -> Result<Vec<Value>, DatabaseError> {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id, dep.entity_type, dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version, dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM deployments dep
                WHERE dep.entity_id = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            Ok(rows.into_iter().map(build_entity_json).collect())
        }

        async fn active_entities_by_prefix(
            &self,
            _prefix: &str,
            _offset: i64,
            _limit: i64,
        ) -> Result<PrefixQueryResult, DatabaseError> {
            Ok(PrefixQueryResult {
                total: 0,
                entities: vec![],
            })
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
            let offset = options.offset.unwrap_or(0);
            let limit = options.limit.unwrap_or(100);
            let fetch_limit = limit + 1;
            let order = options.sorting_order.as_deref().unwrap_or("DESC");

            let mut sql = String::from(
                r#"
                SELECT
                    dep1.id, dep1.entity_type, dep1.entity_id, dep1.entity_pointers,
                    date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
                    dep1.entity_metadata, dep1.deployer_address, dep1.version,
                    dep1.auth_chain,
                    date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
                    dep1.deleter_deployment,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                         FROM content_files cf WHERE cf.deployment = dep1.id),
                        '[]'::json
                    ) AS content_json
                FROM deployments AS dep1
                "#,
            );

            let mut conditions: Vec<String> = Vec::new();
            let mut param_idx: usize = 1;

            if !options.entity_types.is_empty() {
                conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
                param_idx += 1;
            }

            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            let ord = if order == "ASC" { "ASC" } else { "DESC" };
            sql.push_str(&format!(
                " ORDER BY dep1.local_timestamp {ord}, LOWER(dep1.entity_id) {ord} LIMIT ${} OFFSET ${}",
                param_idx,
                param_idx + 1,
            ));

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
                deleter_deployment: Option<i32>,
                content_json: Value,
            }

            let mut query = sqlx::query_as::<_, DepRow>(sqlx::AssertSqlSafe(sql));
            if !options.entity_types.is_empty() {
                query = query.bind(options.entity_types.clone());
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

            let empty_auth = Value::Array(vec![]);
            let deployments: Vec<Value> = rows
                .iter()
                .map(|d| {
                    let content = parse_content_json(&d.content_json);
                    let content_arr: Vec<Value> = content
                        .iter()
                        .map(|(k, h)| json!({"key": k, "hash": h}))
                        .collect();
                    let metadata = d.entity_metadata.as_ref().and_then(|m| m.get("v").cloned());
                    json!({
                        "entityType": d.entity_type,
                        "entityId": d.entity_id,
                        "entityTimestamp": d.entity_timestamp as i64,
                        "pointers": d.entity_pointers,
                        "content": content_arr,
                        "deployedBy": d.deployer_address,
                        "entityVersion": d.version,
                        "auditInfo": {
                            "version": d.version,
                            "authChain": d.auth_chain.as_ref().unwrap_or(&empty_auth),
                            "localTimestamp": d.local_timestamp as i64,
                            "overwrittenBy": null,
                        },
                        "localTimestamp": d.local_timestamp as i64,
                        "metadata": metadata,
                    })
                })
                .collect();

            Ok(DeploymentQueryResult {
                deployments,
                filters: json!({}),
                pagination: PaginationResult {
                    offset,
                    limit,
                    more_data,
                    next: None,
                    last_id: None,
                },
            })
        }

        async fn get_pointer_changes(
            &self,
            _options: &PointerChangesQueryOptions,
        ) -> Result<PointerChangesQueryResult, DatabaseError> {
            Ok(PointerChangesQueryResult {
                deltas: vec![],
                filters: json!({}),
                pagination: PaginationResult {
                    offset: 0,
                    limit: 100,
                    more_data: false,
                    next: None,
                    last_id: None,
                },
            })
        }

        async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
            Ok(vec![])
        }

        async fn get_audit_info(
            &self,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<Option<Value>, DatabaseError> {
            Ok(None)
        }

        async fn find_entity_by_pointer(
            &self,
            pointer: &str,
        ) -> Result<Option<Value>, DatabaseError> {
            let lower = pointer.to_lowercase();
            let pointers = vec![lower];
            let mut entities = self.active_entities_by_pointers(&pointers).await?;
            Ok(entities.pop())
        }
    }

    struct ReadOnlyDeployer;

    #[async_trait]
    impl Deployer for ReadOnlyDeployer {
        async fn deploy_entity(
            &self,
            _files: Vec<Bytes>,
            _entity_id: &str,
            _auth_chain: Value,
            _context: &str,
        ) -> Result<i64, Vec<String>> {
            Err(vec!["read-only".to_string()])
        }
    }

    struct EmptyDenylist;
    impl Denylist for EmptyDenylist {
        fn is_denylisted(&self, _id: &str) -> bool {
            false
        }
    }

    struct StubChallenge;
    impl ChallengeSupervisor for StubChallenge {
        fn get_challenge_text(&self) -> String {
            "bench-challenge".to_string()
        }
    }

    struct StubSyncState;
    impl SynchronizationState for StubSyncState {
        fn get_state(&self) -> String {
            "Syncing".to_string()
        }
    }

    struct StubSnapshots;
    impl SnapshotGenerator for StubSnapshots {
        fn get_current_snapshots(&self) -> Option<Value> {
            None
        }
    }

    struct StubCluster;
    #[async_trait]
    impl ContentCluster for StubCluster {
        fn get_status(&self) -> Value {
            json!({})
        }
    }

    struct StubAcceptingUsers;
    impl AcceptingUsers for StubAcceptingUsers {
        fn is_accepting(&self) -> bool {
            true
        }
    }

    struct BenchState {
        router: axum::Router,
        content_hash: String,
    }

    fn setup() -> BenchState {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db_url = std::env::var("CATALYRST_BENCH_DB_URL")
                .expect("CATALYRST_BENCH_DB_URL not set — point at a populated content DB before running the bench");
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .min_connections(2)
                .connect(&db_url)
                .await
                .expect("Failed to connect to postgres — is the DB running?");

            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .expect("DB connectivity check failed");

            let content_hash: String = sqlx::query_scalar(
                "SELECT content_hash FROM content_files LIMIT 1",
            )
            .fetch_one(&pool)
            .await
            .expect("No content_files rows — is the DB populated?");

            let content_root = std::env::var("CATALYRST_BENCH_CONTENT_ROOT")
                .expect("CATALYRST_BENCH_CONTENT_ROOT not set — point at the content store root");
            let content_storage = catalyrst_storage::ContentStorage::new(&content_root)
            .await
            .expect("Failed to init content storage");

            let state = Arc::new(AppState {
                storage: Arc::new(LiveContentStorage {
                    inner: content_storage,
                }),
                database: Arc::new(LiveDatabase { pool }),
                deployer: Arc::new(ReadOnlyDeployer),
                denylist: Arc::new(EmptyDenylist),
                challenge_supervisor: Arc::new(StubChallenge),
                synchronization_state: Arc::new(StubSyncState),
                snapshot_generator: Arc::new(StubSnapshots),
                content_cluster: Arc::new(StubCluster),
                accepting_users: Arc::new(StubAcceptingUsers),
                deployments_cache: dashmap::DashMap::new(),
                content_version: "bench-0.0.0".to_string(),
                lambdas_version: "bench-0.0.0".to_string(),
                commit_hash: "bench".to_string(),
                eth_network: "mainnet".to_string(),
                content_server_address: "http://127.0.0.1:5141".to_string(),
                read_only: std::sync::atomic::AtomicBool::new(true),
                audit_pool: None,
                content_pool: None,
                entities_cache_control_max_age: 10,
                content_public_url: "http://127.0.0.1:5141/content".to_string(),
                lambdas_public_url: "http://127.0.0.1:5141/lambdas".to_string(),
                realm_name: None,
                squid_pool: None,
                profile_cdn_base_url: String::new(),
                land_image_base_url: String::new(),
            });

            let router = build_router(state);

            BenchState {
                router,
                content_hash,
            }
        })
    }

    pub fn bench_http_handlers(c: &mut Criterion) {
        let bs = setup();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let mut group = c.benchmark_group("http_handlers");
        group.sample_size(50);
        group.measurement_time(std::time::Duration::from_secs(10));

        group.bench_function("GET /status", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let req = Request::builder()
                        .uri("/status")
                        .body(Body::empty())
                        .unwrap();
                    let resp = bs.router.clone().oneshot(req).await.unwrap();
                    assert_eq!(resp.status(), StatusCode::OK);
                    black_box(resp);
                })
            })
        });

        group.bench_function("GET /deployments?profile&limit=5", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let req = Request::builder()
                        .uri("/deployments?entityType=profile&limit=5&sortingOrder=DESC")
                        .body(Body::empty())
                        .unwrap();
                    let resp = bs.router.clone().oneshot(req).await.unwrap();
                    assert_eq!(resp.status(), StatusCode::OK);
                    black_box(resp);
                })
            })
        });

        let contents_uri = format!("/contents/{}", bs.content_hash);
        group.bench_function("GET /contents/<hash>", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let req = Request::builder()
                        .uri(&contents_uri)
                        .body(Body::empty())
                        .unwrap();
                    let resp = bs.router.clone().oneshot(req).await.unwrap();
                    assert_eq!(resp.status(), StatusCode::OK);
                    black_box(resp);
                })
            })
        });

        let etag = format!("\"{}\"", bs.content_hash);
        group.bench_function("GET /contents/<hash> 304 If-None-Match", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let req = Request::builder()
                        .uri(&contents_uri)
                        .header("If-None-Match", &etag)
                        .body(Body::empty())
                        .unwrap();
                    let resp = bs.router.clone().oneshot(req).await.unwrap();
                    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
                    black_box(resp);
                })
            })
        });

        group.bench_function("POST /entities/active pointers=[0,0]", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let req = Request::builder()
                        .method("POST")
                        .uri("/entities/active")
                        .header("Content-Type", "application/json")
                        .body(Body::from(r#"{"pointers":["0,0"]}"#))
                        .unwrap();
                    let resp = bs.router.clone().oneshot(req).await.unwrap();
                    assert_eq!(resp.status(), StatusCode::OK);
                    black_box(resp);
                })
            })
        });

        group.finish();
    }
}

criterion_group!(
    benches,
    bench_hashing,
    bench_json_serialization,
    bench_auth_chain_verification,
    bench_hex_prefix,
    bench_content_hash_validation,
    http_handlers::bench_http_handlers,
);
criterion_main!(benches);
