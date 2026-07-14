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
    // The stream's accounting this entity belongs to, so a loss discovered as late as a failed
    // batch flush can still be attributed to the pass that scheduled the entity instead of
    // contaminating every concurrent stream through the global counter.
    report: Option<Arc<DeploymentReport>>,
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
            // The scheduling side already reported these entities as handed off, so a dropped
            // batch would be a silent loss: nothing re-delivers them and the sync frontier may
            // already be waiting to advance past them. Record every entity of the failed batch
            // in failed_deployments so the retry loop re-deploys them; anything that cannot even
            // be recorded is counted as lost -- globally as a metric, and on the entity's own
            // report, which is what holds that stream's frontier back.
            let failed_store = LiveFailedDeploymentsStore::new(pool.clone());
            // Deduped by entity_id like flush_batch itself, so a duplicate in the batch does not
            // issue a redundant report_failure.
            let mut seen = std::collections::HashSet::with_capacity(entities.len());
            for entity in &entities {
                if !seen.insert(entity.entity_id.as_str()) {
                    continue;
                }
                // An auth chain that no longer round-trips is a silent loss, not a recordable
                // failure: a failed_deployments row with an empty auth chain could never be
                // retried, so writing one would count the entity as handled while guaranteeing
                // it is not.
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
                let failure = FailedDeployment {
                    entity_type: entity.entity_type.clone(),
                    entity_id: entity.entity_id.clone(),
                    reason: FailureReason::DeploymentError,
                    auth_chain,
                    error_description: format!("batch flush failed: {}", e),
                    failure_timestamp: chrono::Utc::now().timestamp_millis(),
                    snapshot_hash: None,
                };
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

    /// Cumulative count of entities this deployer dropped without any durable record: their
    /// batch flush failed AND recording them in failed_deployments failed too. Callers compare
    /// this before and after a drain -- any growth means the sync frontier must not advance,
    /// because nothing will ever re-deliver those entities.
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
        let mut all_pointers: Vec<&String> = entities
            .iter()
            .flat_map(|e| e.entity_pointers.iter())
            .collect();
        all_pointers.sort();
        all_pointers.dedup();
        for p in all_pointers {
            sqlx::query!("SELECT pg_advisory_xact_lock(hashtext($1))", p)
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
        // The OFFSET 0 fence is load-bearing, and entity_type must stay OUTSIDE
        // it: with both predicates inside, the planner may still index-drive the
        // (entity_type, entity_timestamp, entity_id) btree and test && per row -
        // a whole-type walk per batch entity (60s statement timeouts, sync
        // wedged; 2026-08-06, and again 2026-08-11 on a stats-less bootstrap DB
        // where the gin loses on cost). With && as the only sargable predicate
        // inside the fence, candidates can only come from the entity_pointers
        // gin (or a seqscan), regardless of table statistics.
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
    ) -> Result<(), SyncError> {
        let mut parsed = parse_entity_for_deploy(entity_data, entity_id, auth_chain)?;
        parsed.report = report.cloned();

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

// One bounded lookup batch. Upstream (snapshots-fetcher 53e9c07) uses 1000: far under any bind
// limit while keeping realistic passes to a single round trip.
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

    /// Looks up processed snapshots in bounded batches, merging the results -- the port of
    /// upstream's `filterProcessedSnapshotsInChunks`. Serial rather than concurrent on purpose:
    /// the point is to bound the load one decision pass puts on storage, and issuing every
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
}

pub struct LiveFailedDeploymentsStore {
    pool: PgPool,
}

impl LiveFailedDeploymentsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn report_failure(&self, failure: FailedDeployment) -> Result<(), SyncError> {
        let reason = match failure.reason {
            FailureReason::DeploymentError => "Deployment error",
            FailureReason::NoEntity => "No entity",
        };
        sqlx::query!(
            r#"INSERT INTO failed_deployments (entity_id, entity_type, failure_time, reason, auth_chain, error_description, snapshot_hash)
               VALUES ($1, $2, now(), $3, $4::json, $5, $6)
               ON CONFLICT (entity_id) DO UPDATE
               SET failure_time = now(), reason = $3, error_description = $5"#,
            &failure.entity_id,
            &failure.entity_type,
            reason,
            serde_json::to_value(&failure.auth_chain).unwrap_or_else(|_| Value::Array(Vec::new())),
            &failure.error_description,
            failure.snapshot_hash.as_deref().unwrap_or("")
        )
        .execute(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn get_all_failed(&self) -> Result<Vec<FailedDeployment>, SyncError> {
        let rows = sqlx::query!(
            r#"SELECT entity_id, entity_type, reason, auth_chain, error_description, COALESCE(snapshot_hash, '') AS "snapshot_hash!" FROM failed_deployments"#,
        ).fetch_all(&self.pool).await.map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| FailedDeployment {
                entity_id: r.entity_id,
                entity_type: r.entity_type,
                reason: serde_json::from_str(&r.reason).unwrap_or(FailureReason::DeploymentError),
                auth_chain: serde_json::from_value(r.auth_chain).unwrap_or_default(),
                error_description: r.error_description,
                failure_timestamp: 0,
                snapshot_hash: if r.snapshot_hash.is_empty() {
                    None
                } else {
                    Some(r.snapshot_hash)
                },
            })
            .collect())
    }

    pub async fn remove(&self, entity_id: &str) -> Result<(), SyncError> {
        sqlx::query!(
            "DELETE FROM failed_deployments WHERE entity_id = $1",
            entity_id
        )
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
    // The DB integration suite (tests/sync_frontier_monotonic.rs) only runs with a test
    // postgres configured; this pin holds the monotonic upsert shape without one.
    #[test]
    fn sync_frontier_upsert_is_greatest_monotonic() {
        assert!(super::ADVANCE_SYNC_FRONTIER_SQL
            .contains("GREATEST(system_properties.value::bigint, EXCLUDED.value::bigint)"));
        assert!(super::ADVANCE_SYNC_FRONTIER_SQL.contains("ON CONFLICT (key) DO UPDATE"));
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
