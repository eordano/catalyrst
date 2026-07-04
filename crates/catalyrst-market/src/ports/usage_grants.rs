use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct UsageGrantStatus {
    pub urn: String,
    #[serde(rename = "tokenId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub token_id: Option<String>,
    pub category: String,

    pub status: String,

    #[serde(rename = "unlockAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub unlock_at: i64,
}

#[derive(Clone)]
pub struct UsageGrantsComponent {
    pool: Option<PgPool>,
}

impl UsageGrantsComponent {
    pub fn new(pool: Option<PgPool>) -> Self {
        Self { pool }
    }

    pub fn is_enabled(&self) -> bool {
        self.pool.is_some()
    }

    pub async fn get_active_grants_for(&self, owner: &str) -> Vec<UsageGrantStatus> {
        let Some(pool) = &self.pool else {
            return Vec::new();
        };
        let rows = sqlx::query(
            "SELECT urn, token_id, category, unlock_at \
             FROM marketplace.usage_grants \
             WHERE status = 'active' AND grantee_address = lower($1) \
             ORDER BY granted_at DESC",
        )
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        rows.iter()
            .map(|r| UsageGrantStatus {
                urn: r.try_get("urn").unwrap_or_default(),
                token_id: r.try_get("token_id").ok().flatten(),
                category: r.try_get("category").unwrap_or_default(),
                status: "leased".to_string(),
                unlock_at: r
                    .try_get::<chrono::DateTime<chrono::Utc>, _>("unlock_at")
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0),
            })
            .collect()
    }

    pub async fn get_active_grant_urns_for(&self, owner: &str) -> Vec<String> {
        let Some(pool) = &self.pool else {
            return Vec::new();
        };
        let rows = sqlx::query(
            "SELECT DISTINCT urn FROM marketplace.usage_grants \
             WHERE status = 'active' AND grantee_address = lower($1)",
        )
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        rows.iter()
            .filter_map(|r| r.try_get::<String, _>("urn").ok())
            .collect()
    }
}
