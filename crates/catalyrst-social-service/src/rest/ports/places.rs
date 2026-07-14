use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityPlace {
    pub id: String,
    #[serde(rename = "communityId")]
    pub community_id: Uuid,
    #[serde(rename = "addedBy")]
    pub added_by: String,
    #[serde(rename = "addedAt")]
    pub added_at: NaiveDateTime,
}

pub struct PlacesComponent {
    pool: PgPool,
}

impl PlacesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        community_id: Uuid,
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityPlace>, i64), ApiError> {
        let rows = sqlx::query_as::<_, (String, Uuid, String, NaiveDateTime)>(
            "SELECT id, community_id, added_by, added_at FROM community_places \
             WHERE community_id = $1 ORDER BY added_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(community_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM community_places WHERE community_id = $1")
                .bind(community_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let places = rows
            .into_iter()
            .map(|(id, community_id, added_by, added_at)| CommunityPlace {
                id,
                community_id,
                added_by,
                added_at,
            })
            .collect();
        Ok((places, total))
    }
}
