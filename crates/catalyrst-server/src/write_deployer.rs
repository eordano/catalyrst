use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use serde_json::Value;
use sqlx::PgPool;
use tracing::{info, warn};

use catalyrst_crypto::{Eip1654Validator, RpcEip1654Validator, ValidationCache};
use catalyrst_storage::ContentStorage;
use catalyrst_validator::content_validator::{CalculatedHash, ContentValidator, ExternalCalls};
use catalyrst_validator::error::ValidationResponse;
use catalyrst_validator::squid_checker::SquidBlockchainChecker;
use catalyrst_validator::types::{
    AuthChain as VAuthChain, DeploymentAuditInfo, DeploymentToValidate, Entity as VEntity,
};

use crate::state::Deployer;

const DECENTRALAND_ADDRESS: &str = "0x1337e0507eb4ab47e08a179573ed4533d9e22a7b";

#[cfg(test)]
fn happened_before_cmp(a_ts: i64, a_id: &str, b_ts: i64, b_id: &str) -> std::cmp::Ordering {
    a_ts.cmp(&b_ts)
        .then_with(|| a_id.to_lowercase().cmp(&b_id.to_lowercase()))
}

#[cfg(test)]
fn stored_is_newer_or_equal(stored_ts: i64, stored_id: &str, in_ts: i64, in_id: &str) -> bool {
    happened_before_cmp(in_ts, in_id, stored_ts, stored_id) == std::cmp::Ordering::Less
}

#[cfg(test)]
fn stored_is_overwritten(stored_ts: i64, stored_id: &str, in_ts: i64, in_id: &str) -> bool {
    happened_before_cmp(stored_ts, stored_id, in_ts, in_id) == std::cmp::Ordering::Less
}

fn metadata_unchanged(stored_wrapped: Option<&Value>, incoming: Option<&Value>) -> bool {
    let active = match stored_wrapped {
        Some(meta) => meta.get("v").cloned().unwrap_or(Value::Null),
        None => return false,
    };
    let new_meta = incoming.cloned().unwrap_or(Value::Null);
    active == new_meta
}

pub struct LiveExternalCalls {
    storage: Arc<ContentStorage>,
    eip1654: Arc<dyn Eip1654Validator>,
    additional_dcl_address: Option<String>,
}

fn to_crypto_chain(chain: &VAuthChain) -> Result<catalyrst_crypto::AuthChain, String> {
    let v = serde_json::to_value(chain).map_err(|e| e.to_string())?;
    serde_json::from_value(v).map_err(|e| format!("unsupported auth-chain link: {e}"))
}

#[async_trait]
impl ExternalCalls for LiveExternalCalls {
    async fn is_content_stored_already(&self, hashes: &[String]) -> HashMap<String, bool> {
        let mut out = HashMap::with_capacity(hashes.len());
        for h in hashes {
            out.insert(h.clone(), self.storage.exist(h).await.unwrap_or(false));
        }
        out
    }

    async fn fetch_content_file_size(&self, hash: &str) -> Option<usize> {
        let info = self.storage.file_info(hash).await.ok().flatten()?;
        Some(info.content_size.unwrap_or(info.size) as usize)
    }

    async fn validate_signature(
        &self,
        entity_id: &str,
        audit_info: &DeploymentAuditInfo,
        timestamp: i64,
    ) -> Result<(), String> {
        let chain = to_crypto_chain(&audit_info.auth_chain)?;
        catalyrst_crypto::verify::verify_auth_chain_async(
            &chain,
            entity_id,
            Some(timestamp),
            Some(self.eip1654.as_ref()),
        )
        .await
        .map_err(|e| e.to_string())
    }

    fn owner_address(&self, audit_info: &DeploymentAuditInfo) -> String {
        audit_info
            .auth_chain
            .first()
            .map(|l| l.payload.clone())
            .unwrap_or_default()
    }

    fn is_address_owned_by_decentraland(&self, address: &str) -> bool {
        let a = address.to_lowercase();
        a == DECENTRALAND_ADDRESS
            || self
                .additional_dcl_address
                .as_deref()
                .map(|x| x.to_lowercase() == a)
                .unwrap_or(false)
    }

    async fn calculate_files_hashes(
        &self,
        files: &HashMap<String, Vec<u8>>,
    ) -> HashMap<String, CalculatedHash> {
        let handles: Vec<_> = files
            .iter()
            .map(|(name, bytes)| {
                let name = name.clone();
                let bytes = bytes.clone();
                tokio::task::spawn_blocking(move || {
                    let calculated_hash = catalyrst_hashing::hash_bytes_v1(&bytes);
                    (
                        name,
                        CalculatedHash {
                            calculated_hash,
                            buffer: bytes,
                        },
                    )
                })
            })
            .collect();

        let mut out = HashMap::with_capacity(handles.len());
        for handle in handles {
            let (name, calculated) = handle.await.expect("file-hash task panicked");
            out.insert(name, calculated);
        }
        out
    }
}

struct TtlBucket {
    ttl: Duration,
    max_size: usize,
    entries: HashMap<String, Instant>,
}

impl TtlBucket {
    fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            ttl,
            max_size,
            entries: HashMap::new(),
        }
    }

    fn prune(&mut self, now: Instant) {
        let ttl = self.ttl;
        self.entries.retain(|_, t| now.duration_since(*t) < ttl);
    }

    fn is_limited(&mut self, pointers: &[String]) -> bool {
        let now = Instant::now();
        self.prune(now);
        let ttl_hit = pointers.iter().any(|p| self.entries.contains_key(p));
        let size_hit = self.entries.len() > self.max_size;
        ttl_hit || size_hit
    }

    fn record(&mut self, pointers: &[String]) {
        let now = Instant::now();
        for p in pointers {
            self.entries.insert(p.clone(), now);
        }
    }
}

struct DeployRateLimiter {
    buckets: Mutex<HashMap<&'static str, TtlBucket>>,
    unchanged_profile: Mutex<TtlBucket>,
}

impl DeployRateLimiter {
    fn with_reference_defaults() -> Self {
        let mut m = HashMap::new();
        m.insert("profile", TtlBucket::new(Duration::from_secs(3), 500));
        m.insert("scene", TtlBucket::new(Duration::from_secs(20), 1000));
        m.insert("wearable", TtlBucket::new(Duration::from_secs(20), 1000));
        m.insert("store", TtlBucket::new(Duration::from_secs(3), 300));
        m.insert("emote", TtlBucket::new(Duration::from_secs(20), 1000));
        m.insert("outfits", TtlBucket::new(Duration::from_secs(3), 2000));
        Self {
            buckets: Mutex::new(m),
            unchanged_profile: Mutex::new(TtlBucket::new(Duration::from_secs(300), usize::MAX)),
        }
    }

    fn is_rate_limited(&self, etype: &str, pointers: &[String]) -> bool {
        let mut b = self.buckets.lock();
        b.get_mut(etype)
            .map(|bk| bk.is_limited(pointers))
            .unwrap_or(false)
    }

    fn record(&self, etype: &str, pointers: &[String]) {
        if let Some(bk) = self.buckets.lock().get_mut(etype) {
            bk.record(pointers);
        }
    }

    fn is_unchanged_limited(&self, pointers: &[String]) -> bool {
        self.unchanged_profile.lock().is_limited(pointers)
    }

    fn record_unchanged(&self, pointers: &[String]) {
        self.unchanged_profile.lock().record(pointers);
    }
}

pub struct WriteDeployer {
    pool: PgPool,
    storage: Arc<ContentStorage>,
    validator: ContentValidator<LiveExternalCalls, SquidBlockchainChecker>,
    rate_limiter: DeployRateLimiter,
}

impl WriteDeployer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        storage: Arc<ContentStorage>,
        squid_pool: PgPool,
        eth_rpc_url: String,
        ignore_blockchain_access: bool,
        additional_dcl_address: Option<String>,
        tpr_subgraph_url: Option<String>,
        blocks_l2_subgraph_url: Option<String>,
        third_party_root_via_squid: bool,
    ) -> Self {
        let eip1654: Arc<dyn Eip1654Validator> = Arc::new(ValidationCache::new(Arc::new(
            RpcEip1654Validator::new(eth_rpc_url),
        )));

        let external_calls = LiveExternalCalls {
            storage: storage.clone(),
            eip1654,
            additional_dcl_address: additional_dcl_address.clone(),
        };
        let tp_subgraph = match (tpr_subgraph_url, blocks_l2_subgraph_url) {
            (Some(tpr), Some(blocks)) => Some(catalyrst_validator::tp_subgraph::TpSubgraph::new(
                blocks, tpr,
            )),
            _ => None,
        };
        let blockchain_checker = if tp_subgraph.is_some() || third_party_root_via_squid {
            SquidBlockchainChecker::with_third_party(
                squid_pool,
                additional_dcl_address,
                tp_subgraph,
                third_party_root_via_squid,
            )
        } else {
            SquidBlockchainChecker::new(squid_pool, additional_dcl_address)
        };
        let validator =
            ContentValidator::new(external_calls, blockchain_checker, ignore_blockchain_access);

        Self {
            pool,
            storage,
            validator,
            rate_limiter: DeployRateLimiter::with_reference_defaults(),
        }
    }

    async fn is_content_unchanged(&self, entity: &VEntity) -> bool {
        let Some(pointer) = entity.pointers.first() else {
            return false;
        };
        let row: Option<(Option<Value>,)> = sqlx::query_as(
            r#"
            SELECT d.entity_metadata
            FROM active_pointers ap
            JOIN deployments d ON d.entity_id = ap.entity_id
            WHERE ap.pointer = $1
            "#,
        )
        .bind(pointer.to_lowercase())
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();
        let stored = row.and_then(|(m,)| m);
        metadata_unchanged(stored.as_ref(), entity.metadata.as_ref())
    }

    async fn has_newer_entity(&self, entity: &VEntity) -> Result<bool, String> {
        let pointers: Vec<String> = entity.pointers.iter().map(|p| p.to_lowercase()).collect();
        let newer: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT 1
            FROM deployments
            WHERE entity_type = $1
              AND entity_pointers && $2
              AND (entity_timestamp > to_timestamp($3 / 1000.0)
                   OR (entity_timestamp = to_timestamp($3 / 1000.0)
                       AND lower(entity_id) > lower($4)))
            LIMIT 1
            "#,
        )
        .bind(entity.entity_type.as_str())
        .bind(&pointers)
        .bind(entity.timestamp as f64)
        .bind(&entity.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("newer-entity check failed: {e}"))?;
        Ok(newer.is_some())
    }
}

const REQUEST_TTL_BACKWARDS_MS: i64 = 20 * 60 * 1000;
const REQUEST_TTL_FORWARDS_MS: i64 = 15 * 60 * 1000;

#[async_trait]
impl Deployer for WriteDeployer {
    async fn deploy_entity(
        &self,
        files: Vec<Bytes>,
        entity_id: &str,
        auth_chain: Value,
        _context: &str,
    ) -> Result<i64, Vec<String>> {
        let mut by_v0: HashMap<String, Bytes> = HashMap::new();
        let mut by_v1: HashMap<String, Bytes> = HashMap::new();
        for f in &files {
            by_v0.insert(catalyrst_hashing::hash_bytes(f), f.clone());
            by_v1.insert(catalyrst_hashing::hash_bytes_v1(f), f.clone());
        }
        let entity_bytes = by_v1
            .get(entity_id)
            .or_else(|| by_v0.get(entity_id))
            .cloned()
            .ok_or_else(|| {
                vec!["The entity file was not part of the uploaded files.".to_string()]
            })?;

        let mut entity: VEntity = serde_json::from_slice(&entity_bytes)
            .map_err(|e| vec![format!("There was a problem parsing the entity: {e}")])?;

        if entity.id.is_empty() {
            entity.id = entity_id.to_string();
        } else if entity.id != entity_id {
            return Err(vec![
                "Entity id does not match the uploaded entity file.".to_string()
            ]);
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        if now_ms - entity.timestamp > REQUEST_TTL_BACKWARDS_MS {
            return Err(vec![
                "The request is not recent enough, please submit it again with a new timestamp."
                    .to_string(),
            ]);
        }
        if now_ms - entity.timestamp < -REQUEST_TTL_FORWARDS_MS {
            return Err(vec![
                "The request timestamp is too far in the future.".to_string()
            ]);
        }

        let pointers_lc: Vec<String> = entity.pointers.iter().map(|p| p.to_lowercase()).collect();
        let rate_limited_msg = || {
            vec![format!(
                "Entity rate limited (entityId={} pointers={}).",
                entity.id,
                entity.pointers.join(",")
            )]
        };
        if self
            .rate_limiter
            .is_rate_limited(entity.entity_type.as_str(), &pointers_lc)
        {
            return Err(rate_limited_msg());
        }
        let content_unchanged = entity.entity_type
            == catalyrst_validator::types::EntityType::Profile
            && self.is_content_unchanged(&entity).await;
        if content_unchanged && self.rate_limiter.is_unchanged_limited(&pointers_lc) {
            return Err(rate_limited_msg());
        }

        self.rate_limiter
            .record(entity.entity_type.as_str(), &pointers_lc);
        if content_unchanged {
            self.rate_limiter.record_unchanged(&pointers_lc);
        }

        let declared: HashSet<&str> = entity
            .content
            .iter()
            .map(|c| c.hash.as_str())
            .chain(std::iter::once(entity.id.as_str()))
            .collect();
        let mut vfiles: HashMap<String, Vec<u8>> = HashMap::new();
        for f in &files {
            let v1 = catalyrst_hashing::hash_bytes_v1(f);
            let v0 = catalyrst_hashing::hash_bytes(f);
            let key = if declared.contains(v1.as_str()) {
                v1
            } else if declared.contains(v0.as_str()) {
                v0
            } else {
                v1
            };

            if let Some(prev) = vfiles.insert(key.clone(), f.to_vec()) {
                if prev != *f {
                    return Err(vec![format!(
                        "two different uploaded files map to the same content hash {key}"
                    )]);
                }
            }
        }

        let auth_chain: VAuthChain = serde_json::from_value(auth_chain)
            .map_err(|e| vec![format!("invalid auth chain: {e}")])?;
        let audit_info = DeploymentAuditInfo { auth_chain };

        let deployment = DeploymentToValidate {
            entity: entity.clone(),
            files: vfiles,
            audit_info: audit_info.clone(),
        };

        match self.has_newer_entity(&entity).await {
            Ok(true) => {
                return Err(vec![
                    "There is a newer entity pointed by one or more of the pointers you provided."
                        .to_string(),
                ])
            }
            Ok(false) => {}
            Err(e) => return Err(vec![e]),
        }

        match self.validator.validate(&deployment).await {
            ValidationResponse::Ok => {}
            ValidationResponse::Failed { errors } => {
                warn!(entity_id, ?errors, "deployment rejected by validation");
                return Err(errors);
            }
        }

        let creation_ts = self
            .persist(&entity, &entity_bytes, &files, &audit_info)
            .await
            .map_err(|e| vec![e])?;

        Ok(creation_ts)
    }
}

impl WriteDeployer {
    async fn persist(
        &self,
        entity: &VEntity,
        entity_bytes: &Bytes,
        files: &[Bytes],
        audit_info: &DeploymentAuditInfo,
    ) -> Result<i64, String> {
        self.storage
            .store(&entity.id, entity_bytes.clone())
            .await
            .map_err(|e| format!("failed to store entity file: {e}"))?;
        let mut by_v0: HashMap<String, &Bytes> = HashMap::new();
        let mut by_v1: HashMap<String, &Bytes> = HashMap::new();
        for f in files {
            by_v0.insert(catalyrst_hashing::hash_bytes(f), f);
            by_v1.insert(catalyrst_hashing::hash_bytes_v1(f), f);
        }
        for cm in &entity.content {
            if let Some(bytes) = by_v1.get(&cm.hash).or_else(|| by_v0.get(&cm.hash)) {
                self.storage
                    .store(&cm.hash, (*bytes).clone())
                    .await
                    .map_err(|e| format!("failed to store content file {}: {e}", cm.hash))?;
            }
        }

        let deployer_address = audit_info
            .auth_chain
            .first()
            .map(|l| l.payload.clone())
            .unwrap_or_default();
        let metadata = match &entity.metadata {
            Some(m) if !m.is_null() => serde_json::json!({ "v": m }),
            _ => Value::Null,
        };
        let pointers: Vec<String> = entity.pointers.iter().map(|p| p.to_lowercase()).collect();
        let auth_chain_json =
            serde_json::to_value(&audit_info.auth_chain).map_err(|e| e.to_string())?;

        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;

        {
            let mut lock_keys: Vec<&String> = pointers.iter().collect();
            lock_keys.sort();
            lock_keys.dedup();
            for p in lock_keys {
                sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                    .bind(p)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| format!("pointer advisory lock failed: {e}"))?;
            }
        }

        let overwrote: Vec<(i32, Vec<String>)> = sqlx::query_as(
            r#"
            SELECT dep1.id, dep1.entity_pointers
            FROM deployments AS dep1
            LEFT JOIN deployments AS dep2 ON dep1.deleter_deployment = dep2.id
            WHERE dep1.entity_type = $1
              AND dep1.entity_pointers && $2
              AND (dep1.entity_timestamp < to_timestamp($3 / 1000.0)
                   OR (dep1.entity_timestamp = to_timestamp($3 / 1000.0) AND lower(dep1.entity_id) < lower($4)))
              AND (dep2.id IS NULL
                   OR dep2.entity_timestamp > to_timestamp($3 / 1000.0)
                   OR (dep2.entity_timestamp = to_timestamp($3 / 1000.0) AND lower(dep2.entity_id) > lower($4)))
            "#,
        )
        .bind(entity.entity_type.as_str())
        .bind(&pointers)
        .bind(entity.timestamp as f64)
        .bind(&entity.id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("overwrite calculation failed: {e}"))?;

        let dep_id: Option<i32> = sqlx::query_scalar(
            r#"
            INSERT INTO deployments
                (deployer_address, version, entity_type, entity_id, entity_metadata,
                 entity_timestamp, entity_pointers, local_timestamp, auth_chain)
            VALUES ($1, $2, $3, $4, $5, to_timestamp($6 / 1000.0), $7, now(), $8)
            ON CONFLICT (entity_id) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(&deployer_address)
        .bind(&entity.version)
        .bind(entity.entity_type.as_str())
        .bind(&entity.id)
        .bind(&metadata)
        .bind(entity.timestamp as f64)
        .bind(&pointers)
        .bind(&auth_chain_json)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("deployment insert failed: {e}"))?;

        let now_ms = chrono::Utc::now().timestamp_millis();

        let Some(dep_id) = dep_id else {
            tx.commit().await.map_err(|e| e.to_string())?;
            info!(entity_id = %entity.id, "entity already deployed; treating as success");
            return Ok(now_ms);
        };

        if !entity.content.is_empty() {
            let deployments: Vec<i32> = vec![dep_id; entity.content.len()];
            let hashes: Vec<String> = entity.content.iter().map(|c| c.hash.clone()).collect();
            let keys: Vec<String> = entity.content.iter().map(|c| c.file.clone()).collect();
            sqlx::query(
                r#"
                INSERT INTO content_files (deployment, content_hash, key)
                SELECT unnest($1::int[]), unnest($2::text[]), unnest($3::text[])
                "#,
            )
            .bind(&deployments)
            .bind(&hashes)
            .bind(&keys)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("content_files insert failed: {e}"))?;
        }

        if !pointers.is_empty() {
            let entity_ids = vec![entity.id.clone(); pointers.len()];
            let entity_types = vec![entity.entity_type.as_str().to_string(); pointers.len()];

            let has_type_col: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM information_schema.columns
                       WHERE table_schema = current_schema()
                         AND table_name = 'active_pointers'
                         AND column_name = 'entity_type')"#,
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("active_pointers schema probe failed: {e}"))?;

            let upsert = if has_type_col {
                sqlx::query(
                    r#"
                    INSERT INTO active_pointers (pointer, entity_id, entity_type)
                    SELECT unnest($1::text[]), unnest($2::text[]), unnest($3::text[])
                    ON CONFLICT (pointer) DO UPDATE
                        SET entity_id = EXCLUDED.entity_id, entity_type = EXCLUDED.entity_type
                        WHERE NOT EXISTS (
                            SELECT 1 FROM deployments cur
                            WHERE cur.entity_id = active_pointers.entity_id
                              AND (cur.entity_timestamp > to_timestamp($4 / 1000.0)
                                   OR (cur.entity_timestamp = to_timestamp($4 / 1000.0)
                                       AND lower(cur.entity_id) > lower(EXCLUDED.entity_id)))
                        )
                    "#,
                )
                .bind(&pointers)
                .bind(&entity_ids)
                .bind(&entity_types)
                .bind(entity.timestamp as f64)
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO active_pointers (pointer, entity_id)
                    SELECT unnest($1::text[]), unnest($2::text[])
                    ON CONFLICT (pointer) DO UPDATE
                        SET entity_id = EXCLUDED.entity_id
                        WHERE NOT EXISTS (
                            SELECT 1 FROM deployments cur
                            WHERE cur.entity_id = active_pointers.entity_id
                              AND (cur.entity_timestamp > to_timestamp($3 / 1000.0)
                                   OR (cur.entity_timestamp = to_timestamp($3 / 1000.0)
                                       AND lower(cur.entity_id) > lower(EXCLUDED.entity_id)))
                        )
                    "#,
                )
                .bind(&pointers)
                .bind(&entity_ids)
                .bind(entity.timestamp as f64)
            };
            upsert
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("active_pointers upsert failed: {e}"))?;
        }

        let new_set: HashSet<&str> = pointers.iter().map(|p| p.as_str()).collect();
        let mut cleared: Vec<String> = Vec::new();
        for (_, old_pointers) in &overwrote {
            for p in old_pointers {
                if !new_set.contains(p.as_str()) && !cleared.contains(p) {
                    cleared.push(p.clone());
                }
            }
        }
        if !cleared.is_empty() {
            sqlx::query("DELETE FROM active_pointers WHERE pointer = ANY($1)")
                .bind(&cleared)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("active_pointers clear failed: {e}"))?;
        }

        if !overwrote.is_empty() {
            let overwrote_ids: Vec<i32> = overwrote.iter().map(|(id, _)| *id).collect();
            sqlx::query("UPDATE deployments SET deleter_deployment = $1 WHERE id = ANY($2)")
                .bind(dep_id)
                .bind(&overwrote_ids)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("setEntitiesAsOverwritten failed: {e}"))?;
        }

        tx.commit().await.map_err(|e| e.to_string())?;
        info!(
            entity_id = %entity.id,
            dep_id,
            overwrote = overwrote.len(),
            cleared = cleared.len(),
            "deployment committed"
        );
        Ok(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyrst_validator::types::AuthLink as VAuthLink;

    #[test]
    fn auth_chain_bridges_to_crypto_types() {
        let chain: VAuthChain = vec![
            VAuthLink {
                link_type: "SIGNER".into(),
                payload: "0xabc".into(),
                signature: None,
            },
            VAuthLink {
                link_type: "ECDSA_SIGNED_ENTITY".into(),
                payload: "bafkrei...".into(),
                signature: Some("0xdeadbeef".into()),
            },
        ];
        let crypto = to_crypto_chain(&chain).expect("known link types must convert");
        assert_eq!(crypto.len(), 2);

        let eip = vec![VAuthLink {
            link_type: "ECDSA_EIP_1654_SIGNED_ENTITY".into(),
            payload: "p".into(),
            signature: Some("0x00".into()),
        }];
        assert!(to_crypto_chain(&eip).is_ok());

        let bad = vec![VAuthLink {
            link_type: "TOTALLY_BOGUS".into(),
            payload: "p".into(),
            signature: None,
        }];
        assert!(to_crypto_chain(&bad).is_err());
    }

    #[test]
    fn rate_limiter_ttl_and_size() {
        let p = |s: &str| vec![s.to_string()];

        let mut bucket = TtlBucket::new(Duration::from_millis(50), 1000);
        assert!(!bucket.is_limited(&p("0,0")));
        bucket.record(&p("0,0"));
        assert!(bucket.is_limited(&p("0,0")));
        assert!(!bucket.is_limited(&p("1,1")));
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            !bucket.is_limited(&p("0,0")),
            "entry should expire after ttl"
        );

        let mut small = TtlBucket::new(Duration::from_secs(3600), 2);
        small.record(&p("a"));
        small.record(&p("b"));
        small.record(&p("c"));
        assert!(
            small.is_limited(&p("unrelated")),
            "over max_size trips bucket"
        );
    }

    #[test]
    fn has_newer_entity_timestamp_and_tiebreak() {
        assert!(stored_is_newer_or_equal(200, "bafy_a", 100, "bafy_z"));
        assert!(!stored_is_newer_or_equal(50, "bafy_z", 100, "bafy_a"));

        assert!(stored_is_newer_or_equal(100, "bafy_z", 100, "bafy_a"));
        assert!(!stored_is_newer_or_equal(100, "bafy_a", 100, "bafy_z"));

        assert!(!stored_is_newer_or_equal(100, "bafy_m", 100, "bafy_m"));

        assert!(stored_is_newer_or_equal(100, "BAFY_Z", 100, "bafy_a"));
        assert!(!stored_is_newer_or_equal(100, "BAFY_A", 100, "bafy_z"));
    }

    #[test]
    fn newer_and_overwrote_are_consistent_at_the_boundary() {
        let cases = [
            (100i64, "bafy_a", 100i64, "bafy_z"),
            (100, "bafy_z", 100, "bafy_a"),
            (200, "x", 100, "z"),
            (50, "z", 100, "x"),
            (100, "AbC", 100, "abd"),
        ];
        for (sts, sid, its, iid) in cases {
            let newer = stored_is_newer_or_equal(sts, sid, its, iid);
            let overwritten = stored_is_overwritten(sts, sid, its, iid);
            assert!(
                newer ^ overwritten,
                "exactly one of newer/overwritten must hold for ({sts},{sid}) vs ({its},{iid})"
            );
        }
        assert!(!stored_is_newer_or_equal(100, "same", 100, "same"));
        assert!(!stored_is_overwritten(100, "same", 100, "same"));
    }

    #[test]
    fn is_content_unchanged_metadata_compare() {
        let wrapped = serde_json::json!({ "v": { "name": "alice", "n": 1 } });
        let same = serde_json::json!({ "name": "alice", "n": 1 });
        let diff = serde_json::json!({ "name": "bob", "n": 1 });

        assert!(metadata_unchanged(Some(&wrapped), Some(&same)));
        assert!(!metadata_unchanged(Some(&wrapped), Some(&diff)));

        assert!(!metadata_unchanged(None, Some(&same)));

        let wrapped_null = serde_json::json!({ "v": Value::Null });
        assert!(metadata_unchanged(Some(&wrapped_null), None));
        assert!(!metadata_unchanged(Some(&wrapped_null), Some(&same)));
    }

    #[test]
    fn rate_limiter_per_type_defaults() {
        let rl = DeployRateLimiter::with_reference_defaults();
        let ptrs = vec!["0,0".to_string()];
        assert!(!rl.is_rate_limited("scene", &ptrs));
        rl.record("scene", &ptrs);
        assert!(rl.is_rate_limited("scene", &ptrs));
        assert!(!rl.is_rate_limited("profile", &ptrs));
        assert!(!rl.is_rate_limited("bogus", &ptrs));
    }
}
