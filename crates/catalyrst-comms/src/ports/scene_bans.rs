use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

pub struct SceneBansComponent {
    pool: PgPool,
}

impl SceneBansComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn count(&self, place_id: &str) -> Result<i64, ApiError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scene_bans WHERE place_id = $1")
            .bind(place_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }

    pub async fn list_addresses_page(
        &self,
        place_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT banned_address FROM scene_bans WHERE place_id = $1 \
             ORDER BY banned_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(place_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn ban(
        &self,
        place_id: &str,
        banned_address: &str,
        banned_by: &str,
    ) -> Result<Uuid, ApiError> {
        let banned_address = banned_address.to_lowercase();
        let banned_by = banned_by.to_lowercase();
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO scene_bans (place_id, banned_address, banned_by) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (place_id, banned_address) DO UPDATE SET banned_by = EXCLUDED.banned_by, banned_at = now() \
             RETURNING id",
        )
        .bind(place_id)
        .bind(&banned_address)
        .bind(&banned_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn unban(&self, place_id: &str, banned_address: &str) -> Result<u64, ApiError> {
        let banned_address = banned_address.to_lowercase();
        let res = sqlx::query("DELETE FROM scene_bans WHERE place_id = $1 AND banned_address = $2")
            .bind(place_id)
            .bind(&banned_address)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    pub async fn is_banned(&self, place_id: &str, address: &str) -> Result<bool, ApiError> {
        let address = address.to_lowercase();
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM scene_bans WHERE place_id = $1 AND banned_address = $2",
        )
        .bind(place_id)
        .bind(&address)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(n > 0)
    }
}
