use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SnapshotFailedDeployment {
    #[sqlx(rename = "entityId")]
    pub entity_id: String,
    #[sqlx(rename = "entityType")]
    pub entity_type: String,
    #[sqlx(rename = "failureTimestamp")]
    pub failure_timestamp: f64,
    pub reason: String,
    #[sqlx(rename = "authChain")]
    pub auth_chain: serde_json::Value,
    #[sqlx(rename = "errorDescription")]
    pub error_description: String,
    #[sqlx(rename = "snapshotHash")]
    pub snapshot_hash: String,
}

pub async fn save_snapshot_failed_deployment<'e, E: sqlx::Executor<'e, Database = Postgres>>(
    executor: E,
    fd: &SnapshotFailedDeployment,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO failed_deployments
            (entity_id, entity_type, failure_time, reason, auth_chain, error_description, snapshot_hash)
        VALUES
            ($1, $2, to_timestamp($3 / 1000.0), $4, $5::json, $6, $7)
        RETURNING entity_id
        "#,
    )
    .bind(&fd.entity_id)
    .bind(&fd.entity_type)
    .bind(fd.failure_timestamp)
    .bind(&fd.reason)
    .bind(fd.auth_chain.to_string())
    .bind(&fd.error_description)
    .bind(&fd.snapshot_hash)
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn delete_failed_deployment(
    pool: &PgPool,
    entity_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM failed_deployments WHERE entity_id = $1")
        .bind(entity_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_snapshot_failed_deployments(
    pool: &PgPool,
) -> Result<Vec<SnapshotFailedDeployment>, sqlx::Error> {
    sqlx::query_as::<_, SnapshotFailedDeployment>(
        r#"
        SELECT
            entity_id AS "entityId",
            entity_type AS "entityType",
            date_part('epoch', failure_time) * 1000 AS "failureTimestamp",
            reason,
            auth_chain AS "authChain",
            error_description AS "errorDescription",
            snapshot_hash AS "snapshotHash"
        FROM failed_deployments
        "#,
    )
    .fetch_all(pool)
    .await
}
