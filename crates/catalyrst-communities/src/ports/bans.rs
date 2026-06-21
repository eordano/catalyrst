use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityBan {
    #[serde(rename = "communityId")]
    pub community_id: Uuid,
    #[serde(rename = "memberAddress")]
    pub banned_address: String,
    #[serde(rename = "bannedBy")]
    pub banned_by: String,
    #[serde(rename = "bannedAt")]
    pub banned_at: NaiveDateTime,
    pub reason: Option<String>,
}

pub struct BansComponent {
    pool: PgPool,
}

impl BansComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        community_id: Uuid,
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityBan>, i64), ApiError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, NaiveDateTime, Option<String>)>(
            "SELECT community_id, banned_address, banned_by, banned_at, reason \
             FROM community_bans WHERE community_id = $1 AND active = TRUE \
             ORDER BY banned_at ASC LIMIT $2 OFFSET $3",
        )
        .bind(community_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM community_bans WHERE community_id = $1 AND active = TRUE",
        )
        .bind(community_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let bans = rows
            .into_iter()
            .map(
                |(community_id, banned_address, banned_by, banned_at, reason)| CommunityBan {
                    community_id,
                    banned_address,
                    banned_by,
                    banned_at,
                    reason,
                },
            )
            .collect();
        Ok((bans, total))
    }
}
