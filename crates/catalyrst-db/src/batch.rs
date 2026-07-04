use sqlx::PgPool;
use tracing::debug;

use crate::active_entities_repository;
use crate::content_files_repository;
use crate::deployments_repository::DeploymentRow;

pub const DEFAULT_BATCH_SIZE: usize = 200;

#[derive(Debug, Clone)]
struct PendingContentFile {
    deployment_index: usize,
    pub key: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct PointerUpdate {
    pub pointer: String,
    pub entity_id: String,
    pub entity_type: String,
}

#[derive(Debug, Clone)]
pub enum PointerOp {
    Set(PointerUpdate),
    Clear(String),
}

pub struct BatchDeploymentPipeline {
    pool: PgPool,
    batch_size: usize,

    pending_deployments: Vec<DeploymentRow>,
    pending_content_files: Vec<PendingContentFile>,
    pending_pointer_ops: Vec<PointerOp>,
}

impl BatchDeploymentPipeline {
    pub fn new(pool: PgPool, batch_size: usize) -> Self {
        let batch_size = if batch_size == 0 {
            DEFAULT_BATCH_SIZE
        } else {
            batch_size
        };
        Self {
            pool,
            batch_size,
            pending_deployments: Vec::with_capacity(batch_size),
            pending_content_files: Vec::with_capacity(batch_size * 4),
            pending_pointer_ops: Vec::with_capacity(batch_size * 2),
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending_deployments.len()
    }

    pub async fn add(
        &mut self,
        deployment: DeploymentRow,
        content_files: Vec<(String, String)>,
        pointer_ops: Vec<PointerOp>,
    ) -> Result<(), sqlx::Error> {
        let idx = self.pending_deployments.len();
        self.pending_deployments.push(deployment);

        for (key, hash) in content_files {
            self.pending_content_files.push(PendingContentFile {
                deployment_index: idx,
                key,
                content_hash: hash,
            });
        }

        self.pending_pointer_ops.extend(pointer_ops);

        if self.pending_deployments.len() >= self.batch_size {
            self.flush().await?;
        }

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), sqlx::Error> {
        if self.pending_deployments.is_empty() {
            return Ok(());
        }

        let deployment_count = self.pending_deployments.len();
        debug!(
            "Flushing batch: {} deployments, {} content files, {} pointer ops",
            deployment_count,
            self.pending_content_files.len(),
            self.pending_pointer_ops.len(),
        );

        let mut tx = self.pool.begin().await?;

        let n = self.pending_deployments.len();

        let mut deployer_addresses: Vec<&str> = Vec::with_capacity(n);
        let mut versions: Vec<&str> = Vec::with_capacity(n);
        let mut entity_types: Vec<&str> = Vec::with_capacity(n);
        let mut entity_ids: Vec<&str> = Vec::with_capacity(n);
        let mut entity_timestamps: Vec<f64> = Vec::with_capacity(n);
        let mut entity_metadatas: Vec<Option<serde_json::Value>> = Vec::with_capacity(n);
        let mut local_timestamps: Vec<f64> = Vec::with_capacity(n);
        let mut auth_chains: Vec<&str> = Vec::with_capacity(n);
        let mut deleter_deployments: Vec<Option<i32>> = Vec::with_capacity(n);

        let pointer_jsons: Vec<serde_json::Value> = self
            .pending_deployments
            .iter()
            .map(|d| {
                serde_json::Value::Array(
                    d.entity_pointers
                        .iter()
                        .map(|p| serde_json::Value::String(p.clone()))
                        .collect(),
                )
            })
            .collect();

        for d in &self.pending_deployments {
            deployer_addresses.push(&d.deployer_address);
            versions.push(&d.version);
            entity_types.push(&d.entity_type);
            entity_ids.push(&d.entity_id);
            entity_timestamps.push(d.entity_timestamp_ms);
            entity_metadatas.push(
                d.entity_metadata
                    .as_ref()
                    .map(|m| serde_json::json!({ "v": m })),
            );
            local_timestamps.push(d.local_timestamp_ms);
            auth_chains.push(&d.auth_chain_json);
            deleter_deployments.push(d.overwritten_by);
        }

        let id_rows: Vec<(i32,)> = sqlx::query_as(
            r#"
            INSERT INTO deployments
                (deployer_address, version, entity_type, entity_id, entity_timestamp,
                 entity_pointers, entity_metadata, local_timestamp, auth_chain, deleter_deployment)
            SELECT d.deployer_address, d.version, d.entity_type, d.entity_id,
                   d.entity_timestamp,
                   ARRAY(SELECT jsonb_array_elements_text(d.ptrs_json)),
                   d.entity_metadata::json, d.local_timestamp, d.auth_chain::json,
                   d.deleter_deployment
            FROM (
                SELECT unnest($1::text[]) AS deployer_address,
                       unnest($2::text[]) AS version,
                       unnest($3::text[]) AS entity_type,
                       unnest($4::text[]) AS entity_id,
                       to_timestamp(unnest($5::float8[]) / 1000.0) AS entity_timestamp,
                       unnest($6::jsonb[]) AS ptrs_json,
                       unnest($7::jsonb[]) AS entity_metadata,
                       to_timestamp(unnest($8::float8[]) / 1000.0) AS local_timestamp,
                       unnest($9::text[]) AS auth_chain,
                       unnest($10::int4[]) AS deleter_deployment
            ) d
            RETURNING id
            "#,
        )
        .bind(&deployer_addresses)
        .bind(&versions)
        .bind(&entity_types)
        .bind(&entity_ids)
        .bind(&entity_timestamps)
        .bind(&pointer_jsons)
        .bind(&entity_metadatas)
        .bind(&local_timestamps)
        .bind(&auth_chains)
        .bind(&deleter_deployments)
        .fetch_all(&mut *tx)
        .await?;

        let generated_ids: Vec<i32> = id_rows.into_iter().map(|r| r.0).collect();

        if !self.pending_content_files.is_empty() {
            let cf_tuples: Vec<(i32, &str, &str)> = self
                .pending_content_files
                .iter()
                .map(|cf| {
                    let dep_id = generated_ids[cf.deployment_index];
                    (dep_id, cf.content_hash.as_str(), cf.key.as_str())
                })
                .collect();

            content_files_repository::batch_save_content_files(&mut *tx, &cf_tuples).await?;
        }

        let mut upserts: Vec<(&str, &str, &str)> = Vec::new();
        let mut clears: Vec<&str> = Vec::new();

        for op in &self.pending_pointer_ops {
            match op {
                PointerOp::Set(update) => {
                    upserts.push((
                        update.pointer.as_str(),
                        update.entity_id.as_str(),
                        update.entity_type.as_str(),
                    ));
                }
                PointerOp::Clear(ptr) => {
                    clears.push(ptr.as_str());
                }
            }
        }

        if !upserts.is_empty() {
            active_entities_repository::batch_update_active_pointers(&mut *tx, &upserts).await?;
        }

        if !clears.is_empty() {
            active_entities_repository::batch_clear_active_pointers(&mut *tx, &clears).await?;
        }

        if !generated_ids.is_empty() {
            let batch_ids: Vec<i32> = generated_ids.clone();
            let batch_entity_types: Vec<&str> = self
                .pending_deployments
                .iter()
                .map(|d| d.entity_type.as_str())
                .collect();
            let batch_entity_ids: Vec<&str> = self
                .pending_deployments
                .iter()
                .map(|d| d.entity_id.as_str())
                .collect();
            let batch_timestamps: Vec<f64> = self
                .pending_deployments
                .iter()
                .map(|d| d.entity_timestamp_ms)
                .collect();

            let batch_pointers: Vec<serde_json::Value> = self
                .pending_deployments
                .iter()
                .map(|d| {
                    serde_json::Value::Array(
                        d.entity_pointers
                            .iter()
                            .map(|p| serde_json::Value::String(p.clone()))
                            .collect(),
                    )
                })
                .collect();

            sqlx::query(
                r#"
                UPDATE deployments AS old
                SET deleter_deployment = batch.new_id
                FROM (
                    SELECT d.new_id, d.etype, d.new_eid, d.new_ts,
                           ARRAY(SELECT jsonb_array_elements_text(d.ptrs_json)) AS ptrs
                    FROM (
                        SELECT unnest($1::int4[]) AS new_id,
                               unnest($2::text[]) AS etype,
                               unnest($3::text[]) AS new_eid,
                               to_timestamp(unnest($4::float8[]) / 1000.0) AS new_ts,
                               unnest($5::jsonb[]) AS ptrs_json
                    ) d
                ) AS batch
                WHERE old.entity_type = batch.etype
                  AND old.entity_pointers && batch.ptrs
                  AND old.deleter_deployment IS NULL
                  AND old.id <> batch.new_id
                  AND (old.entity_timestamp < batch.new_ts
                       OR (old.entity_timestamp = batch.new_ts AND old.entity_id < batch.new_eid))
                "#,
            )
            .bind(&batch_ids)
            .bind(&batch_entity_types)
            .bind(&batch_entity_ids)
            .bind(&batch_timestamps)
            .bind(&batch_pointers)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        self.pending_deployments.clear();
        self.pending_content_files.clear();
        self.pending_pointer_ops.clear();

        debug!(
            "Batch flush complete: {} deployments committed",
            deployment_count
        );

        Ok(())
    }
}
