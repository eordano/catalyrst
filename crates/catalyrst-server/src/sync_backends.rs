use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::{Mutex, Semaphore};

use catalyrst_sync::{
    AuthChain, CatalystServerInfo, ContentStorage as SyncContentStorage, DaoClient,
    Deployer as SyncDeployer, DeploymentContext, DeploymentRepository, FailedDeployment,
    FailedDeploymentsStore, FailureReason, ProcessedSnapshotStore, SnapshotStorageCheck, SyncError,
    Timestamp,
};

pub struct LiveSyncStorage {
    inner: catalyrst_storage::ContentStorage,
}

impl LiveSyncStorage {
    pub fn new(inner: catalyrst_storage::ContentStorage) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl SyncContentStorage for LiveSyncStorage {
    async fn exists(&self, hash: &str) -> Result<bool, SyncError> {
        self.inner
            .exist(hash)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))
    }

    async fn store(&self, hash: &str, data: Bytes) -> Result<(), SyncError> {
        self.inner
            .store(hash, data)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))
    }

    async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, SyncError> {
        self.inner
            .retrieve(hash)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))
    }

    async fn delete(&self, hashes: &[String]) -> Result<(), SyncError> {
        for hash in hashes {
            let _ = self.inner.delete(hash).await;
        }
        Ok(())
    }
}

struct ParsedEntity {
    deployer_address: String,
    version: String,
    entity_type: String,
    entity_id: String,
    entity_metadata: Value,
    entity_timestamp: f64,
    entity_pointers: Vec<String>,
    auth_chain: Value,
    content: Vec<(String, String)>,
}

fn parse_entity_for_deploy(
    entity_data: &[u8],
    entity_id: &str,
    auth_chain: &AuthChain,
) -> Result<ParsedEntity, SyncError> {
    let entity: Value = serde_json::from_slice(entity_data)?;

    let pointers: Vec<String> = entity
        .get("pointers")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let timestamp = entity
        .get("timestamp")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.0);
    let entity_type = entity
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown")
        .to_string();
    let version = entity
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("v3")
        .to_string();

    let deployer = auth_chain
        .first()
        .map(|link| link.payload.clone())
        .unwrap_or_default();

    let content: Vec<(String, String)> = entity
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let file = c.get("file").and_then(|f| f.as_str());
                    let hash = c.get("hash").and_then(|h| h.as_str());
                    match (file, hash) {
                        (Some(f), Some(h)) => Some((f.to_string(), h.to_string())),
                        _ => None,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let auth_chain_json =
        serde_json::to_value(auth_chain).map_err(|e| SyncError::Storage(e.to_string()))?;
    let metadata = match entity.get("metadata") {
        Some(m) if !m.is_null() => serde_json::json!({"v": m}),
        _ => Value::Null,
    };

    Ok(ParsedEntity {
        deployer_address: deployer,
        version,
        entity_type,
        entity_id: entity_id.to_string(),
        entity_metadata: metadata,
        entity_timestamp: timestamp,
        entity_pointers: pointers,
        auth_chain: auth_chain_json,
        content,
    })
}

const BATCH_SIZE: usize = 500;
const BATCH_TIMEOUT_MS: u64 = 200;

fn flush_concurrency() -> usize {
    std::env::var("SYNC_FLUSH_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(8)
}

pub struct LiveSyncDeployer {
    pool: PgPool,
    batch: Arc<Mutex<Vec<ParsedEntity>>>,

    flush_sem: Arc<Semaphore>,
    in_flight: Arc<std::sync::atomic::AtomicUsize>,
    idle_notify: Arc<tokio::sync::Notify>,
}

fn spawn_flush(
    pool: PgPool,
    flush_sem: Arc<Semaphore>,
    in_flight: Arc<std::sync::atomic::AtomicUsize>,
    idle_notify: Arc<tokio::sync::Notify>,
    entities: Vec<ParsedEntity>,
) {
    if entities.is_empty() {
        return;
    }
    in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    tokio::spawn(async move {
        let _permit = flush_sem.acquire().await;
        if let Err(e) = flush_batch(&pool, entities).await {
            tracing::error!(error = %e, "Batch flush failed");
        }
        in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        idle_notify.notify_waiters();
    });
}

impl LiveSyncDeployer {
    pub fn new(pool: PgPool) -> Self {
        let deployer = Self {
            pool: pool.clone(),
            batch: Arc::new(Mutex::new(Vec::with_capacity(BATCH_SIZE))),
            flush_sem: Arc::new(Semaphore::new(flush_concurrency())),
            in_flight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            idle_notify: Arc::new(tokio::sync::Notify::new()),
        };

        let batch = deployer.batch.clone();
        let pool2 = deployer.pool.clone();
        let sem = deployer.flush_sem.clone();
        let in_flight = deployer.in_flight.clone();
        let idle = deployer.idle_notify.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(BATCH_TIMEOUT_MS)).await;
                let entities: Vec<ParsedEntity> = {
                    let mut buf = batch.lock().await;
                    if buf.is_empty() {
                        continue;
                    }
                    std::mem::take(&mut *buf)
                };
                spawn_flush(
                    pool2.clone(),
                    sem.clone(),
                    in_flight.clone(),
                    idle.clone(),
                    entities,
                );
            }
        });

        deployer
    }
}

async fn flush_batch(pool: &PgPool, entities: Vec<ParsedEntity>) -> Result<(), SyncError> {
    if entities.is_empty() {
        return Ok(());
    }

    let entities: Vec<ParsedEntity> = {
        let mut seen = std::collections::HashSet::with_capacity(entities.len());
        entities
            .into_iter()
            .filter(|e| seen.insert(e.entity_id.clone()))
            .collect()
    };

    let count = entities.len();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;

    {
        let mut all_pointers: Vec<&String> = entities
            .iter()
            .flat_map(|e| e.entity_pointers.iter())
            .collect();
        all_pointers.sort();
        all_pointers.dedup();
        for p in all_pointers {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                .bind(p)
                .execute(&mut *tx)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
        }
    }

    let mut deployer_addrs: Vec<String> = Vec::with_capacity(count);
    let mut versions: Vec<String> = Vec::with_capacity(count);
    let mut entity_types: Vec<String> = Vec::with_capacity(count);
    let mut entity_ids: Vec<String> = Vec::with_capacity(count);
    let mut metadatas: Vec<Value> = Vec::with_capacity(count);
    let mut timestamps: Vec<f64> = Vec::with_capacity(count);
    let mut pointers_json: Vec<Value> = Vec::with_capacity(count);
    let mut auth_chains: Vec<Value> = Vec::with_capacity(count);

    for e in &entities {
        deployer_addrs.push(e.deployer_address.clone());
        versions.push(e.version.clone());
        entity_types.push(e.entity_type.clone());
        entity_ids.push(e.entity_id.clone());
        metadatas.push(e.entity_metadata.clone());
        timestamps.push(e.entity_timestamp);
        pointers_json.push(Value::Array(
            e.entity_pointers
                .iter()
                .map(|p| Value::String(p.clone()))
                .collect(),
        ));
        auth_chains.push(e.auth_chain.clone());
    }

    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        INSERT INTO deployments
            (deployer_address, version, entity_type, entity_id, entity_metadata,
             entity_timestamp, entity_pointers, local_timestamp, auth_chain)
        SELECT da, v, et, ei, em,
               to_timestamp(ts / 1000.0),
               ARRAY(SELECT json_array_elements_text(ep)),
               now(), ac
        FROM unnest(
            $1::text[], $2::text[], $3::text[], $4::text[],
            $5::json[], $6::float8[], $7::json[], $8::json[]
        ) AS t(da, v, et, ei, em, ts, ep, ac)
        ON CONFLICT (entity_id) DO NOTHING
        RETURNING entity_id, id
        "#,
    )
    .bind(&deployer_addrs)
    .bind(&versions)
    .bind(&entity_types)
    .bind(&entity_ids)
    .bind(&metadatas)
    .bind(&timestamps)
    .bind(&pointers_json)
    .bind(&auth_chains)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| SyncError::Storage(e.to_string()))?;

    if rows.is_empty() {
        tx.commit()
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        return Ok(());
    }

    let id_map: std::collections::HashMap<&str, i32> =
        rows.iter().map(|(eid, id)| (eid.as_str(), *id)).collect();

    let mut cf_deployments: Vec<i32> = Vec::new();
    let mut cf_hashes: Vec<String> = Vec::new();
    let mut cf_keys: Vec<String> = Vec::new();

    let mut ap_dedup: std::collections::HashMap<String, (String, String, f64)> =
        std::collections::HashMap::new();

    for e in entities.iter() {
        let Some(&dep_id) = id_map.get(e.entity_id.as_str()) else {
            continue;
        };

        for (key, hash) in &e.content {
            cf_deployments.push(dep_id);
            cf_hashes.push(hash.clone());
            cf_keys.push(key.clone());
        }

        for ptr in &e.entity_pointers {
            let replace = match ap_dedup.get(ptr) {
                None => true,
                Some((existing_id, _, existing_ts)) => {
                    e.entity_timestamp > *existing_ts
                        || (e.entity_timestamp == *existing_ts && e.entity_id > *existing_id)
                }
            };
            if replace {
                ap_dedup.insert(
                    ptr.clone(),
                    (
                        e.entity_id.clone(),
                        e.entity_type.clone(),
                        e.entity_timestamp,
                    ),
                );
            }
        }
    }

    let mut ap_pointers: Vec<String> = Vec::with_capacity(ap_dedup.len());
    let mut ap_entity_ids: Vec<String> = Vec::with_capacity(ap_dedup.len());
    let mut ap_entity_types: Vec<String> = Vec::with_capacity(ap_dedup.len());

    for (ptr, (eid, etype, _)) in ap_dedup {
        ap_pointers.push(ptr);
        ap_entity_ids.push(eid);
        ap_entity_types.push(etype);
    }

    if !cf_deployments.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO content_files (deployment, content_hash, key)
            SELECT unnest($1::int[]), unnest($2::text[]), unnest($3::text[])
            "#,
        )
        .bind(&cf_deployments)
        .bind(&cf_hashes)
        .bind(&cf_keys)
        .execute(&mut *tx)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
    }

    if !ap_pointers.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO active_pointers (pointer, entity_id, entity_type)
            SELECT unnest($1::text[]), unnest($2::text[]), unnest($3::text[])
            ON CONFLICT (pointer) DO UPDATE
                SET entity_id = EXCLUDED.entity_id, entity_type = EXCLUDED.entity_type
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM deployments cur, deployments incoming
                    WHERE cur.entity_id = active_pointers.entity_id
                      AND incoming.entity_id = EXCLUDED.entity_id
                      AND (cur.entity_timestamp > incoming.entity_timestamp
                           OR (cur.entity_timestamp = incoming.entity_timestamp
                               AND lower(cur.entity_id) > lower(EXCLUDED.entity_id)))
                )
            "#,
        )
        .bind(&ap_pointers)
        .bind(&ap_entity_ids)
        .bind(&ap_entity_types)
        .execute(&mut *tx)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
    }

    tx.commit()
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;

    metrics::counter!("catalyrst_sync_deployments_total").increment(rows.len() as u64);
    tracing::info!(
        count = rows.len(),
        batch_size = count,
        "Batch flush committed"
    );
    Ok(())
}

#[async_trait]
impl SyncDeployer for LiveSyncDeployer {
    async fn deploy_entity(
        &self,
        entity_data: &[u8],
        entity_id: &str,
        auth_chain: &AuthChain,
        _context: DeploymentContext,
    ) -> Result<(), SyncError> {
        let parsed = parse_entity_for_deploy(entity_data, entity_id, auth_chain)?;

        let entities_to_flush = {
            let mut buf = self.batch.lock().await;
            buf.push(parsed);
            if buf.len() >= BATCH_SIZE {
                Some(std::mem::take(&mut *buf))
            } else {
                None
            }
        };

        if let Some(entities) = entities_to_flush {
            spawn_flush(
                self.pool.clone(),
                self.flush_sem.clone(),
                self.in_flight.clone(),
                self.idle_notify.clone(),
                entities,
            );
        }

        Ok(())
    }

    async fn flush(&self) -> Result<(), SyncError> {
        let entities: Vec<ParsedEntity> = {
            let mut buf = self.batch.lock().await;
            std::mem::take(&mut *buf)
        };
        spawn_flush(
            self.pool.clone(),
            self.flush_sem.clone(),
            self.in_flight.clone(),
            self.idle_notify.clone(),
            entities,
        );

        loop {
            let notified = self.idle_notify.notified();
            if self.in_flight.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                return Ok(());
            }
            notified.await;
        }
    }
}

pub struct LiveProcessedSnapshotStore {
    pool: PgPool,
}

impl LiveProcessedSnapshotStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProcessedSnapshotStore for LiveProcessedSnapshotStore {
    async fn filter_processed(
        &self,
        hashes: &[String],
    ) -> Result<std::collections::HashSet<String>, SyncError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT hash FROM processed_snapshots WHERE hash = ANY($1)")
                .bind(hashes)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(rows.into_iter().map(|(h,)| h).collect())
    }

    async fn mark_processed(&self, hash: &str) -> Result<(), SyncError> {
        sqlx::query("INSERT INTO processed_snapshots (hash, process_time) VALUES ($1, now()) ON CONFLICT DO NOTHING")
            .bind(hash).execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct LiveSnapshotStorageCheck {
    snapshot_storage: catalyrst_storage::SnapshotStorage,
}

impl LiveSnapshotStorageCheck {
    pub fn new(snapshot_storage: catalyrst_storage::SnapshotStorage) -> Self {
        Self { snapshot_storage }
    }
}

#[async_trait]
impl SnapshotStorageCheck for LiveSnapshotStorageCheck {
    async fn has(&self, snapshot_hash: &str) -> Result<bool, SyncError> {
        self.snapshot_storage
            .exist(snapshot_hash)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))
    }
}

pub struct LiveFailedDeploymentsStore {
    pool: PgPool,
}

impl LiveFailedDeploymentsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FailedDeploymentsStore for LiveFailedDeploymentsStore {
    async fn report_failure(&self, failure: FailedDeployment) -> Result<(), SyncError> {
        let reason = match failure.reason {
            FailureReason::DeploymentError => "Deployment error",
            FailureReason::NoEntity => "No entity",
        };
        sqlx::query(
            r#"INSERT INTO failed_deployments (entity_id, entity_type, failure_time, reason, auth_chain, error_description, snapshot_hash)
               VALUES ($1, $2, now(), $3, $4::json, $5, $6)
               ON CONFLICT (entity_id) DO UPDATE
               SET failure_time = now(), reason = $3, error_description = $5"#,
        )
        .bind(&failure.entity_id)
        .bind(&failure.entity_type)
        .bind(reason)
        .bind(serde_json::to_string(&failure.auth_chain).unwrap_or_else(|_| "[]".to_string()))
        .bind(&failure.error_description)
        .bind(failure.snapshot_hash.as_deref().unwrap_or(""))
        .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn find_failed(&self, entity_id: &str) -> Result<Option<FailedDeployment>, SyncError> {
        let row: Option<(String, String, String, serde_json::Value, String, String)> = sqlx::query_as(
            "SELECT entity_id, entity_type, reason, auth_chain, error_description, COALESCE(snapshot_hash, '') FROM failed_deployments WHERE entity_id = $1",
        ).bind(entity_id).fetch_optional(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(
            row.map(|(id, etype, reason, ac, desc, snap)| FailedDeployment {
                entity_id: id,
                entity_type: etype,
                reason: serde_json::from_str(&reason).unwrap_or(FailureReason::DeploymentError),
                auth_chain: serde_json::from_value(ac).unwrap_or_default(),
                error_description: desc,
                failure_timestamp: 0,
                snapshot_hash: if snap.is_empty() { None } else { Some(snap) },
            }),
        )
    }

    async fn get_all_failed(&self) -> Result<Vec<FailedDeployment>, SyncError> {
        let rows: Vec<(String, String, String, serde_json::Value, String, String)> = sqlx::query_as(
            "SELECT entity_id, entity_type, reason, auth_chain, error_description, COALESCE(snapshot_hash, '') FROM failed_deployments",
        ).fetch_all(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(id, etype, reason, ac, desc, snap)| FailedDeployment {
                entity_id: id,
                entity_type: etype,
                reason: serde_json::from_str(&reason).unwrap_or(FailureReason::DeploymentError),
                auth_chain: serde_json::from_value(ac).unwrap_or_default(),
                error_description: desc,
                failure_timestamp: 0,
                snapshot_hash: if snap.is_empty() { None } else { Some(snap) },
            })
            .collect())
    }

    async fn remove(&self, entity_id: &str) -> Result<(), SyncError> {
        sqlx::query("DELETE FROM failed_deployments WHERE entity_id = $1")
            .bind(entity_id)
            .execute(&self.pool)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct SyncGauges {
    pub frontier_ms: Arc<std::sync::atomic::AtomicI64>,
    pub heartbeat_ms: Arc<std::sync::atomic::AtomicI64>,
}

pub struct LiveDeploymentRepository {
    pool: PgPool,
    gauges: SyncGauges,
}

impl LiveDeploymentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            gauges: SyncGauges::default(),
        }
    }

    pub fn with_gauges(pool: PgPool, gauges: SyncGauges) -> Self {
        Self { pool, gauges }
    }

    pub async fn load_all_entity_ids(&self) -> Result<Vec<String>, SyncError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT entity_id FROM deployments")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

#[async_trait]
impl DeploymentRepository for LiveDeploymentRepository {
    async fn is_entity_deployed(
        &self,
        entity_id: &str,
        timestamp_ms: Timestamp,
    ) -> Result<bool, SyncError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM deployments WHERE entity_id = $1 AND entity_timestamp >= to_timestamp($2 / 1000.0))",
        ).bind(entity_id).bind(timestamp_ms as f64)
         .fetch_one(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(exists)
    }

    async fn get_sync_frontier(&self) -> Result<Timestamp, SyncError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_properties WHERE key = 'sync_frontier'")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
        let ts = row
            .and_then(|(v,)| v.parse::<Timestamp>().ok())
            .unwrap_or(0);
        if ts > 0 {
            self.gauges
                .frontier_ms
                .store(ts, std::sync::atomic::Ordering::Relaxed);
            metrics::gauge!("catalyrst_sync_frontier_timestamp_seconds").set(ts as f64 / 1000.0);
        }
        Ok(ts)
    }

    async fn set_sync_frontier(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        sqlx::query(
            "INSERT INTO system_properties (key, value) VALUES ('sync_frontier', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
        ).bind(timestamp.to_string())
         .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        self.gauges
            .frontier_ms
            .store(timestamp, std::sync::atomic::Ordering::Relaxed);
        metrics::gauge!("catalyrst_sync_frontier_timestamp_seconds").set(timestamp as f64 / 1000.0);
        Ok(())
    }

    async fn set_sync_heartbeat(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        sqlx::query(
            "INSERT INTO system_properties (key, value) VALUES ('sync_heartbeat', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
        ).bind(timestamp.to_string())
         .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        self.gauges
            .heartbeat_ms
            .store(timestamp, std::sync::atomic::Ordering::Relaxed);
        metrics::gauge!("catalyrst_sync_heartbeat_timestamp_seconds")
            .set(timestamp as f64 / 1000.0);
        Ok(())
    }

    async fn resolve_deleter_deployments(&self) -> Result<(), SyncError> {
        let start = std::time::Instant::now();
        let result = sqlx::query(
            r#"
            UPDATE deployments older
            SET deleter_deployment = newer.id
            FROM deployments newer
            WHERE older.deleter_deployment IS NULL
              AND newer.entity_type = older.entity_type
              AND newer.entity_id != older.entity_id
              AND newer.entity_pointers && older.entity_pointers
              AND newer.deleter_deployment IS NULL
              AND (newer.entity_timestamp > older.entity_timestamp
                   OR (newer.entity_timestamp = older.entity_timestamp
                       AND newer.entity_id > older.entity_id))
              AND NOT EXISTS (
                  SELECT 1 FROM deployments mid
                  WHERE mid.entity_type = older.entity_type
                    AND mid.entity_id != older.entity_id
                    AND mid.entity_id != newer.entity_id
                    AND mid.entity_pointers && older.entity_pointers
                    AND mid.deleter_deployment IS NULL
                    AND (mid.entity_timestamp > older.entity_timestamp
                         OR (mid.entity_timestamp = older.entity_timestamp
                             AND mid.entity_id > older.entity_id))
                    AND (mid.entity_timestamp < newer.entity_timestamp
                         OR (mid.entity_timestamp = newer.entity_timestamp
                             AND mid.entity_id < newer.entity_id))
              )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;

        tracing::info!(
            rows_affected = result.rows_affected(),
            elapsed_ms = start.elapsed().as_millis() as u64,
            "Resolved deleter_deployment"
        );
        Ok(())
    }
}

pub struct StaticPeerList {
    servers: Vec<CatalystServerInfo>,
}

impl StaticPeerList {
    pub fn from_custom(urls: &str) -> Self {
        let servers = urls
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .enumerate()
            .map(|(i, address)| CatalystServerInfo {
                address: address.trim_end_matches('/').to_string(),
                owner: String::new(),
                id: format!("peer-{}", i),
            })
            .collect();
        Self { servers }
    }

    pub fn from_env() -> Self {
        let custom_dao = std::env::var("CUSTOM_DAO").unwrap_or_default();
        Self::parse(custom_dao)
    }

    fn parse(custom_dao: String) -> Self {
        let servers = custom_dao
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .enumerate()
            .map(|(i, address)| {
                let content_url = if address.ends_with("/content") {
                    address.clone()
                } else {
                    format!("{}/content", address.trim_end_matches('/'))
                };
                CatalystServerInfo {
                    address: content_url,
                    owner: String::new(),
                    id: format!("peer-{}", i),
                }
            })
            .collect();
        Self { servers }
    }
}

#[async_trait]
impl DaoClient for StaticPeerList {
    async fn get_all_content_servers(&self) -> Result<Vec<CatalystServerInfo>, SyncError> {
        Ok(self.servers.clone())
    }
}
