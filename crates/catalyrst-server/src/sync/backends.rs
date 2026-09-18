use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::{Mutex, Semaphore};

use super::batch_deployer::DeploymentReport;
use super::{AuthChain, DeploymentContext, FailedDeployment, FailureReason, SyncError, Timestamp};

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
    report: Option<Arc<DeploymentReport>>,
    /// Retry state the entity must be recorded with if the batch flush that carries it fails.
    /// The deployer is asynchronous -- `deploy_entity` returns once the entity is buffered, so
    /// the caller's attempt is already spent by the time a flush failure re-reports it, and
    /// reporting a fresh 0/0 here would rewind the backoff the retry worker had accrued.
    /// The wait stays relative: the deadline is stamped when the failure is recorded, not when
    /// the attempt was dispatched, so a slow attempt does not eat the wait it just earned.
    retry_count: u32,
    backoff_ms: u64,
}

fn flush_failure_record(
    entity: &ParsedEntity,
    auth_chain: AuthChain,
    error_description: String,
    retry: (u32, u64),
) -> FailedDeployment {
    let now_ms = chrono::Utc::now().timestamp_millis();
    FailedDeployment {
        entity_type: entity.entity_type.clone(),
        entity_id: entity.entity_id.clone(),
        reason: FailureReason::DeploymentError,
        auth_chain,
        error_description,
        failure_timestamp: now_ms,
        snapshot_hash: None,
        retry_count: retry.0,
        next_retry_at: now_ms + retry.1 as i64,
    }
}

/// Strongest retry state per entity id in a batch. One batch can carry the same entity twice --
/// the sync path keeps re-encountering exactly the entity the retry worker keeps re-attempting --
/// and the single record a failed flush writes has to be the stronger sighting, or the accrued
/// attempts are lost to whichever copy happened to be buffered first.
fn strongest_retry_state(entities: &[ParsedEntity]) -> HashMap<&str, (u32, u64)> {
    let mut strongest: HashMap<&str, (u32, u64)> = HashMap::with_capacity(entities.len());
    for entity in entities {
        let slot = strongest
            .entry(entity.entity_id.as_str())
            .or_insert((entity.retry_count, entity.backoff_ms));
        slot.0 = slot.0.max(entity.retry_count);
        slot.1 = slot.1.max(entity.backoff_ms);
    }
    strongest
}

fn reason_label(reason: &FailureReason) -> &'static str {
    match reason {
        FailureReason::DeploymentError => "Deployment error",
        FailureReason::NoEntity => "No entity",
    }
}

fn reason_from_label(label: &str) -> FailureReason {
    match label {
        "No entity" => FailureReason::NoEntity,
        _ => FailureReason::DeploymentError,
    }
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
        report: None,
        retry_count: 0,
        backoff_ms: 0,
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

#[derive(Clone)]
pub struct LiveSyncDeployer {
    pool: PgPool,
    batch: Arc<Mutex<Vec<ParsedEntity>>>,

    flush_sem: Arc<Semaphore>,
    in_flight: Arc<std::sync::atomic::AtomicUsize>,
    idle_notify: Arc<tokio::sync::Notify>,
    lost: Arc<std::sync::atomic::AtomicU64>,
}

fn spawn_flush(
    pool: PgPool,
    flush_sem: Arc<Semaphore>,
    in_flight: Arc<std::sync::atomic::AtomicUsize>,
    idle_notify: Arc<tokio::sync::Notify>,
    lost: Arc<std::sync::atomic::AtomicU64>,
    entities: Vec<ParsedEntity>,
) {
    if entities.is_empty() {
        return;
    }
    in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    tokio::spawn(async move {
        let _permit = flush_sem.acquire().await;
        if let Err(e) = flush_batch(&pool, &entities).await {
            tracing::error!(error = %e, count = entities.len(), "Batch flush failed");
            let failed_store = LiveFailedDeploymentsStore::new(pool.clone());
            let mut strongest = strongest_retry_state(&entities);
            for entity in &entities {
                let Some(retry) = strongest.remove(entity.entity_id.as_str()) else {
                    continue;
                };
                let auth_chain: AuthChain = match serde_json::from_value(entity.auth_chain.clone())
                {
                    Ok(chain) => chain,
                    Err(conv_err) => {
                        lost.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if let Some(report) = &entity.report {
                            report.record_lost();
                        }
                        tracing::error!(
                            entity_id = %entity.entity_id,
                            error = %conv_err,
                            "Entity dropped by a failed batch flush has an unconvertible auth \
                             chain; counting it as lost instead of recording an un-retryable row"
                        );
                        continue;
                    }
                };
                let failure = flush_failure_record(
                    entity,
                    auth_chain,
                    format!("batch flush failed: {}", e),
                    retry,
                );
                if let Err(record_err) = failed_store.report_failure(failure).await {
                    lost.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if let Some(report) = &entity.report {
                        report.record_lost();
                    }
                    tracing::error!(
                        entity_id = %entity.entity_id,
                        error = %record_err,
                        "Entity dropped by a failed batch flush could not be recorded as a \
                         failed deployment; holding the sync frontier back"
                    );
                }
            }
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
            lost: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };

        let background = deployer.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(BATCH_TIMEOUT_MS)).await;
                let entities: Vec<ParsedEntity> = {
                    let mut buf = background.batch.lock().await;
                    if buf.is_empty() {
                        continue;
                    }
                    std::mem::take(&mut *buf)
                };
                background.trigger_flush(entities);
            }
        });

        deployer
    }

    fn trigger_flush(&self, entities: Vec<ParsedEntity>) {
        spawn_flush(
            self.pool.clone(),
            self.flush_sem.clone(),
            self.in_flight.clone(),
            self.idle_notify.clone(),
            self.lost.clone(),
            entities,
        );
    }

    /// Entities this deployer dropped without any durable record: the batch flush failed AND
    /// recording them in failed_deployments failed too. Callers compare this before and after a
    /// drain -- any growth means the sync frontier must not advance, because nothing will
    /// re-deliver those entities.
    pub fn lost_count(&self) -> u64 {
        self.lost.load(std::sync::atomic::Ordering::SeqCst)
    }
}

async fn flush_batch(pool: &PgPool, entities: &[ParsedEntity]) -> Result<(), SyncError> {
    if entities.is_empty() {
        return Ok(());
    }

    let entities: Vec<&ParsedEntity> = {
        let mut seen = std::collections::HashSet::with_capacity(entities.len());
        entities
            .iter()
            .filter(|e| seen.insert(e.entity_id.clone()))
            .collect()
    };

    let count = entities.len();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;

    {
        let all_pointers: Vec<String> = entities
            .iter()
            .flat_map(|e| e.entity_pointers.iter().cloned())
            .collect();
        crate::write_deployer::lock_deployment_pointers(&mut tx, &all_pointers)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
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

    let rows = sqlx::query!(
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
        &deployer_addrs,
        &versions,
        &entity_types,
        &entity_ids,
        &metadatas,
        &timestamps,
        &pointers_json,
        &auth_chains
    )
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
        rows.iter().map(|r| (r.entity_id.as_str(), r.id)).collect();

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
        sqlx::query!(
            r#"
            INSERT INTO content_files (deployment, content_hash, key)
            SELECT unnest($1::int[]), unnest($2::text[]), unnest($3::text[])
            "#,
            &cf_deployments,
            &cf_hashes,
            &cf_keys
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
    }

    let mut ow_ids: Vec<i32> = Vec::with_capacity(rows.len());
    let mut ow_types: Vec<String> = Vec::with_capacity(rows.len());
    let mut ow_eids: Vec<String> = Vec::with_capacity(rows.len());
    let mut ow_ts: Vec<f64> = Vec::with_capacity(rows.len());
    let mut ow_ptrs: Vec<Value> = Vec::with_capacity(rows.len());
    for e in &entities {
        let Some(&dep_id) = id_map.get(e.entity_id.as_str()) else {
            continue;
        };
        ow_ids.push(dep_id);
        ow_types.push(e.entity_type.clone());
        ow_eids.push(e.entity_id.clone());
        ow_ts.push(e.entity_timestamp);
        ow_ptrs.push(Value::Array(
            e.entity_pointers
                .iter()
                .map(|p| Value::String(p.clone()))
                .collect(),
        ));
    }

    sqlx::query!(
        r#"
        UPDATE deployments AS old
        SET deleter_deployment = batch.new_id
        FROM (
            SELECT d.new_id, d.etype, d.new_eid, d.new_ts,
                   ARRAY(SELECT json_array_elements_text(d.ptrs_json)) AS ptrs
            FROM (
                SELECT unnest($1::int4[]) AS new_id,
                       unnest($2::text[]) AS etype,
                       unnest($3::text[]) AS new_eid,
                       to_timestamp(unnest($4::float8[]) / 1000.0) AS new_ts,
                       unnest($5::json[]) AS ptrs_json
            ) d
        ) AS batch
        WHERE old.entity_type = batch.etype
          AND old.entity_pointers && batch.ptrs
          AND old.deleter_deployment IS NULL
          AND old.id <> batch.new_id
          AND (old.entity_timestamp < batch.new_ts
               OR (old.entity_timestamp = batch.new_ts AND old.entity_id < batch.new_eid))
        "#,
        &ow_ids,
        &ow_types,
        &ow_eids,
        &ow_ts,
        &ow_ptrs
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| SyncError::Storage(e.to_string()))?;

    sqlx::query!(
        r#"
        UPDATE deployments AS n
        SET deleter_deployment = sub.newer_id
        FROM (
            SELECT nr.id,
                   (SELECT c.id FROM (
                        SELECT d2.id, d2.entity_timestamp, d2.entity_id, d2.entity_type
                        FROM deployments d2
                        WHERE d2.entity_pointers && nr.entity_pointers
                          AND d2.id <> nr.id
                          AND (d2.entity_timestamp > nr.entity_timestamp
                               OR (d2.entity_timestamp = nr.entity_timestamp
                                   AND d2.entity_id > nr.entity_id))
                        OFFSET 0
                    ) c
                    WHERE c.entity_type = nr.entity_type
                    ORDER BY c.entity_timestamp, c.entity_id
                    LIMIT 1) AS newer_id
            FROM deployments nr
            WHERE nr.id = ANY($1)
        ) sub
        WHERE n.id = sub.id
          AND n.deleter_deployment IS NULL
          AND sub.newer_id IS NOT NULL
        "#,
        &ow_ids
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| SyncError::Storage(e.to_string()))?;

    sqlx::query!(
        r#"
        DELETE FROM active_pointers AS ap
        USING deployments d
        WHERE ap.entity_id = d.entity_id
          AND d.deleter_deployment IS NOT NULL
          AND (d.deleter_deployment = ANY($1) OR d.id = ANY($1))
        "#,
        &ow_ids
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| SyncError::Storage(e.to_string()))?;

    if !ap_pointers.is_empty() {
        sqlx::query!(
            r#"
            INSERT INTO active_pointers (pointer, entity_id, entity_type)
            SELECT t.pointer, t.entity_id, t.entity_type
            FROM unnest($1::text[], $2::text[], $3::text[]) AS t(pointer, entity_id, entity_type)
            JOIN deployments dep ON dep.entity_id = t.entity_id
            WHERE dep.deleter_deployment IS NULL
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
            &ap_pointers,
            &ap_entity_ids,
            &ap_entity_types
        )
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

impl LiveSyncDeployer {
    pub async fn deploy_entity(
        &self,
        entity_data: &[u8],
        entity_id: &str,
        auth_chain: &AuthChain,
        _context: DeploymentContext,
        report: Option<&Arc<DeploymentReport>>,
        retry_state: Option<(u32, u64)>,
    ) -> Result<(), SyncError> {
        let mut parsed = parse_entity_for_deploy(entity_data, entity_id, auth_chain)?;
        parsed.report = report.cloned();
        if let Some((retry_count, backoff_ms)) = retry_state {
            parsed.retry_count = retry_count;
            parsed.backoff_ms = backoff_ms;
        }

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
            self.trigger_flush(entities);
        }

        Ok(())
    }

    pub async fn flush(&self) -> Result<(), SyncError> {
        let entities: Vec<ParsedEntity> = {
            let mut buf = self.batch.lock().await;
            std::mem::take(&mut *buf)
        };
        self.trigger_flush(entities);

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

const PROCESSED_SNAPSHOT_LOOKUP_CHUNK_SIZE: usize = 1000;

impl LiveProcessedSnapshotStore {
    pub async fn filter_processed(
        &self,
        hashes: &[String],
    ) -> Result<std::collections::HashSet<String>, SyncError> {
        let rows = sqlx::query_scalar!(
            "SELECT hash FROM processed_snapshots WHERE hash = ANY($1)",
            hashes
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(rows.into_iter().collect())
    }

    /// Port of upstream's `filterProcessedSnapshotsInChunks`. Serial rather than concurrent on
    /// purpose: the point is to bound the load one decision pass puts on storage, and issuing every
    /// chunk at once would keep the peak it is meant to remove.
    pub async fn filter_processed_in_chunks(
        &self,
        hashes: impl IntoIterator<Item = String>,
    ) -> Result<std::collections::HashSet<String>, SyncError> {
        let mut processed = std::collections::HashSet::new();
        let mut chunk: Vec<String> = Vec::with_capacity(PROCESSED_SNAPSHOT_LOOKUP_CHUNK_SIZE);
        for hash in hashes {
            chunk.push(hash);
            if chunk.len() == PROCESSED_SNAPSHOT_LOOKUP_CHUNK_SIZE {
                processed.extend(self.filter_processed(&chunk).await?);
                chunk.clear();
            }
        }
        if !chunk.is_empty() {
            processed.extend(self.filter_processed(&chunk).await?);
        }
        Ok(processed)
    }

    pub async fn mark_processed(&self, hash: &str) -> Result<(), SyncError> {
        sqlx::query!("INSERT INTO processed_snapshots (hash, process_time) VALUES ($1, now()) ON CONFLICT DO NOTHING", hash)
            .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn mark_processed_many(&self, hashes: &[String]) -> Result<(), SyncError> {
        if hashes.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO processed_snapshots (hash, process_time)
             SELECT h, now() FROM unnest($1::text[]) AS h
             ON CONFLICT DO NOTHING",
        )
        .bind(hashes)
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }
}

pub struct LiveFailedDeploymentsStore {
    pool: PgPool,
}

impl LiveFailedDeploymentsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// GREATEST-monotonic on the retry columns: the sync path reports a failure with a fresh
    /// entity's zeroed retry state every time an entity reappears in a snapshot, and that must
    /// never rewind a backoff deadline or attempt count the retry worker already advanced.
    pub async fn report_failure(&self, failure: FailedDeployment) -> Result<(), SyncError> {
        let reason = reason_label(&failure.reason);
        sqlx::query!(
            r#"INSERT INTO failed_deployments (entity_id, entity_type, failure_time, reason, auth_chain, error_description, snapshot_hash, retry_count, next_retry_at)
               VALUES ($1, $2, to_timestamp($9::double precision / 1000.0), $3, $4::json, $5, $6, $7, to_timestamp($8::double precision / 1000.0))
               ON CONFLICT (entity_id) DO UPDATE
               SET failure_time = EXCLUDED.failure_time, reason = $3, error_description = $5,
                   retry_count = GREATEST(failed_deployments.retry_count, EXCLUDED.retry_count),
                   next_retry_at = GREATEST(failed_deployments.next_retry_at, EXCLUDED.next_retry_at)"#,
            &failure.entity_id,
            &failure.entity_type,
            reason,
            serde_json::to_value(&failure.auth_chain).unwrap_or_else(|_| Value::Array(Vec::new())),
            &failure.error_description,
            failure.snapshot_hash.as_deref().unwrap_or(""),
            failure.retry_count as i32,
            failure.next_retry_at as f64,
            failure.failure_timestamp as f64
        )
        .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn get_all_failed(&self) -> Result<Vec<FailedDeployment>, SyncError> {
        let rows = sqlx::query!(
            r#"SELECT entity_id, entity_type, reason, auth_chain, error_description, COALESCE(snapshot_hash, '') AS "snapshot_hash!", retry_count, date_part('epoch', next_retry_at) * 1000 AS "next_retry_at!", date_part('epoch', failure_time) * 1000 AS "failure_timestamp!" FROM failed_deployments"#,
        ).fetch_all(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| FailedDeployment {
                entity_id: r.entity_id,
                entity_type: r.entity_type,
                reason: reason_from_label(&r.reason),
                auth_chain: serde_json::from_value(r.auth_chain).unwrap_or_default(),
                error_description: r.error_description,
                failure_timestamp: r.failure_timestamp as i64,
                snapshot_hash: if r.snapshot_hash.is_empty() {
                    None
                } else {
                    Some(r.snapshot_hash)
                },
                retry_count: r.retry_count.max(0) as u32,
                next_retry_at: r.next_retry_at as i64,
            })
            .collect())
    }

    /// Clears the row of an entity that deployed successfully, pinned to the attempt count the
    /// caller read. The deployer is asynchronous, so a flush failure for the same entity can have
    /// committed a report at a higher count between the read and this DELETE, and that report is
    /// then the only record of a real failure: dropping it would lose the evidence, the accrued
    /// backoff and the entity's progress towards the cap. Mirrors the `remove_exhausted` guard.
    pub async fn remove(&self, entity_id: &str, max_retry_count: u32) -> Result<(), SyncError> {
        sqlx::query!(
            "DELETE FROM failed_deployments WHERE entity_id = $1 AND retry_count <= $2",
            entity_id,
            max_retry_count as i32
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Drops the entries that have burned through the retry cap, answering which ones actually
    /// went. The `retry_count >= $2` guard is the point of the call: a give-up decision taken
    /// against an earlier read must not delete a row that a successful deployment cleared and a
    /// fresh failure has since re-created with a lower count. Chunked so one give-up pass over a
    /// large backlog never grows into a single unbounded statement; a chunk that fails leaves its
    /// entries in place for the next cycle.
    pub async fn remove_exhausted(
        &self,
        entity_ids: &[String],
        min_retry_count: u32,
    ) -> Result<Vec<String>, SyncError> {
        let mut removed = Vec::new();
        for chunk in entity_ids.chunks(EXHAUSTED_DELETE_BATCH_SIZE) {
            let rows = sqlx::query!(
                r#"DELETE FROM failed_deployments WHERE entity_id = ANY($1::text[]) AND retry_count >= $2 RETURNING entity_id"#,
                chunk,
                min_retry_count as i32
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
            removed.extend(rows.into_iter().map(|r| r.entity_id));
        }
        Ok(removed)
    }
}

/// Entity ids per give-up DELETE. `ANY($1)` binds the whole chunk as one parameter, so this is
/// not about the bind-parameter ceiling: it bounds each statement's payload, lock footprint and
/// duration.
const EXHAUSTED_DELETE_BATCH_SIZE: usize = 1000;

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
        let rows = sqlx::query_scalar!("SELECT entity_id FROM deployments")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(rows)
    }

    pub async fn is_entity_deployed(
        &self,
        entity_id: &str,
        timestamp_ms: Timestamp,
    ) -> Result<bool, SyncError> {
        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM deployments WHERE entity_id = $1 AND entity_timestamp >= to_timestamp($2 / 1000.0)) AS "exists!""#,
            entity_id,
            timestamp_ms as f64
        )
         .fetch_one(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(exists)
    }

    pub async fn get_sync_frontier(&self) -> Result<Timestamp, SyncError> {
        let row =
            sqlx::query_scalar!("SELECT value FROM system_properties WHERE key = 'sync_frontier'")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
        let ts = row.and_then(|v| v.parse::<Timestamp>().ok()).unwrap_or(0);
        if ts > 0 {
            self.gauges
                .frontier_ms
                .store(ts, std::sync::atomic::Ordering::Relaxed);
            metrics::gauge!("catalyrst_sync_frontier_timestamp_seconds").set(ts as f64 / 1000.0);
        }
        Ok(ts)
    }

    pub async fn set_sync_frontier(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        sqlx::query!(
            "INSERT INTO system_properties (key, value) VALUES ('sync_frontier', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
            timestamp.to_string()
        )
         .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        self.gauges
            .frontier_ms
            .store(timestamp, std::sync::atomic::Ordering::Relaxed);
        metrics::gauge!("catalyrst_sync_frontier_timestamp_seconds").set(timestamp as f64 / 1000.0);
        Ok(())
    }

    pub async fn advance_sync_frontier(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        sqlx::query(ADVANCE_SYNC_FRONTIER_SQL)
            .bind(timestamp.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        let previous = self
            .gauges
            .frontier_ms
            .fetch_max(timestamp, std::sync::atomic::Ordering::Relaxed);
        let current = previous.max(timestamp);
        metrics::gauge!("catalyrst_sync_frontier_timestamp_seconds").set(current as f64 / 1000.0);
        Ok(())
    }

    /// The server's own persisted resume point, or None when the server has never had a
    /// confirmed durable point (or migration 0004 has not been applied yet -- the table's
    /// absence degrades to the pre-cursor global-frontier resume instead of failing sync).
    pub async fn get_server_sync_cursor(
        &self,
        server_url: &str,
    ) -> Result<Option<Timestamp>, SyncError> {
        let row = sqlx::query_scalar!(
            "SELECT cursor_ms FROM server_sync_cursors WHERE server_url = $1",
            server_url
        )
        .fetch_optional(&self.pool)
        .await;
        match row {
            Ok(r) => Ok(r),
            Err(e) if cursor_table_missing(&e) => {
                warn_cursor_table_missing();
                Ok(None)
            }
            Err(e) => Err(SyncError::Storage(e.to_string())),
        }
    }

    /// GREATEST-monotonic advance of one server's own cursor, the per-server counterpart of
    /// `advance_sync_frontier`: a stale offer can never rewind the server's persisted resume
    /// point. A missing table (migration 0004 unapplied) is a no-op, not an error.
    pub async fn advance_server_sync_cursor(
        &self,
        server_url: &str,
        timestamp: Timestamp,
    ) -> Result<(), SyncError> {
        match sqlx::query(ADVANCE_SERVER_SYNC_CURSOR_SQL)
            .bind(server_url)
            .bind(timestamp)
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if cursor_table_missing(&e) => {
                warn_cursor_table_missing();
                Ok(())
            }
            Err(e) => Err(SyncError::Storage(e.to_string())),
        }
    }

    pub async fn set_sync_heartbeat(&self, timestamp: Timestamp) -> Result<(), SyncError> {
        sqlx::query!(
            "INSERT INTO system_properties (key, value) VALUES ('sync_heartbeat', $1) ON CONFLICT (key) DO UPDATE SET value = $1",
            timestamp.to_string()
        )
         .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        self.gauges
            .heartbeat_ms
            .store(timestamp, std::sync::atomic::Ordering::Relaxed);
        metrics::gauge!("catalyrst_sync_heartbeat_timestamp_seconds")
            .set(timestamp as f64 / 1000.0);
        Ok(())
    }

    pub async fn resolve_deleter_deployments(&self) -> Result<(), SyncError> {
        let start = std::time::Instant::now();
        let result = sqlx::query!(
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

/// GREATEST-monotonic upsert: a stale offer can never rewind the persisted frontier.
const ADVANCE_SYNC_FRONTIER_SQL: &str = "INSERT INTO system_properties (key, value) VALUES ('sync_frontier', $1) ON CONFLICT (key) DO UPDATE SET value = GREATEST(system_properties.value::bigint, EXCLUDED.value::bigint)::text";

/// GREATEST-monotonic upsert of one server's own cursor (migration 0004): the per-server
/// resume state has the same never-rewinds guarantee as the global frontier.
const ADVANCE_SERVER_SYNC_CURSOR_SQL: &str = "INSERT INTO server_sync_cursors (server_url, cursor_ms, updated_at) VALUES ($1, $2, now()) ON CONFLICT (server_url) DO UPDATE SET cursor_ms = GREATEST(server_sync_cursors.cursor_ms, EXCLUDED.cursor_ms), updated_at = now()";

/// Undefined-table (42P01): migration 0004 not applied. The cursor methods degrade to the
/// pre-cursor behavior instead of failing sync on it.
fn cursor_table_missing(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("42P01"))
}

fn warn_cursor_table_missing() {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "server_sync_cursors table missing (migration 0004 not applied); per-server sync \
             cursors are inactive and bootstrap resume falls back to the global frontier"
        );
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn sync_frontier_upsert_is_greatest_monotonic() {
        assert!(super::ADVANCE_SYNC_FRONTIER_SQL
            .contains("GREATEST(system_properties.value::bigint, EXCLUDED.value::bigint)"));
        assert!(super::ADVANCE_SYNC_FRONTIER_SQL.contains("ON CONFLICT (key) DO UPDATE"));
    }

    const ENTITY_BODY: &[u8] = br#"{"type":"scene","pointers":["0,0"],"timestamp":1,"content":[]}"#;

    #[test]
    fn a_flush_failure_record_carries_the_stamped_retry_state() {
        let entity = super::parse_entity_for_deploy(ENTITY_BODY, "bafytest", &Vec::new()).unwrap();
        let before = chrono::Utc::now().timestamp_millis();

        let fresh = super::flush_failure_record(&entity, Vec::new(), String::new(), (0, 0));
        assert_eq!(fresh.retry_count, 0);
        assert!(fresh.next_retry_at >= before);

        let record =
            super::flush_failure_record(&entity, Vec::new(), "boom".to_string(), (4, 1_800_000));
        assert_eq!(record.retry_count, 4);
        assert!(record.next_retry_at >= before + 1_800_000);
        assert!(record.next_retry_at <= chrono::Utc::now().timestamp_millis() + 1_800_000);
        assert_eq!(record.entity_id, "bafytest");
        assert_eq!(record.error_description, "boom");
    }

    #[test]
    fn a_repeated_entity_in_one_batch_is_recorded_with_its_strongest_retry_state() {
        let weak = super::parse_entity_for_deploy(ENTITY_BODY, "bafydup", &Vec::new()).unwrap();
        let mut strong =
            super::parse_entity_for_deploy(ENTITY_BODY, "bafydup", &Vec::new()).unwrap();
        strong.retry_count = 4;
        strong.backoff_ms = 1_800_000;
        let entities = vec![weak, strong];

        let mut strongest = super::strongest_retry_state(&entities);
        assert_eq!(strongest.len(), 1);

        let before = chrono::Utc::now().timestamp_millis();
        let retry = strongest
            .remove(entities[0].entity_id.as_str())
            .expect("the first sighting takes the merged state");
        let record =
            super::flush_failure_record(&entities[0], Vec::new(), "boom".to_string(), retry);
        assert_eq!(record.retry_count, 4);
        assert!(record.next_retry_at >= before + 1_800_000);
        assert!(strongest.remove(entities[1].entity_id.as_str()).is_none());
    }

    #[tokio::test]
    async fn a_buffered_entity_carries_the_retry_state_of_the_attempt_that_queued_it() {
        let deployer = super::LiveSyncDeployer {
            pool: sqlx::PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none").unwrap(),
            batch: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            flush_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            in_flight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            idle_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
            lost: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        let entity = ENTITY_BODY;

        deployer
            .deploy_entity(
                entity,
                "bafysync",
                &Vec::new(),
                crate::sync::DeploymentContext::Synced,
                None,
                None,
            )
            .await
            .unwrap();
        deployer
            .deploy_entity(
                entity,
                "bafyretry",
                &Vec::new(),
                crate::sync::DeploymentContext::SyncedFix,
                None,
                Some((4, 1_800_000)),
            )
            .await
            .unwrap();

        let buffered = deployer.batch.lock().await;
        assert_eq!(buffered[0].retry_count, 0);
        assert_eq!(buffered[0].backoff_ms, 0);
        assert_eq!(buffered[1].retry_count, 4);
        assert_eq!(buffered[1].backoff_ms, 1_800_000);
    }

    #[test]
    fn a_failure_reason_survives_the_column_round_trip() {
        use crate::sync::FailureReason;
        for reason in [FailureReason::DeploymentError, FailureReason::NoEntity] {
            let label = super::reason_label(&reason);
            assert_eq!(super::reason_from_label(label), reason);
        }
        assert_eq!(super::reason_label(&FailureReason::NoEntity), "No entity");
        assert_eq!(
            super::reason_label(&FailureReason::DeploymentError),
            "Deployment error"
        );
    }

    #[test]
    fn server_sync_cursor_upsert_is_greatest_monotonic() {
        assert!(super::ADVANCE_SERVER_SYNC_CURSOR_SQL
            .contains("GREATEST(server_sync_cursors.cursor_ms, EXCLUDED.cursor_ms)"));
        assert!(
            super::ADVANCE_SERVER_SYNC_CURSOR_SQL.contains("ON CONFLICT (server_url) DO UPDATE")
        );
    }
}
