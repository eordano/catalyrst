use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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
    #[sqlx(rename = "retryCount")]
    pub retry_count: i32,
    #[sqlx(rename = "nextRetryAt")]
    pub next_retry_at: f64,
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
            snapshot_hash AS "snapshotHash",
            retry_count AS "retryCount",
            date_part('epoch', next_retry_at) * 1000 AS "nextRetryAt"
        FROM failed_deployments
        "#,
    )
    .fetch_all(pool)
    .await
}
