use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

pub struct SceneAdminComponent {
    pool: PgPool,
}

impl SceneAdminComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_admin_addresses(&self, place_id: &str) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT admin FROM scene_admin \
             WHERE place_id = $1 AND active = TRUE \
             ORDER BY created_at DESC",
        )
        .bind(place_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add(&self, place_id: &str, admin: &str, added_by: &str) -> Result<Uuid, ApiError> {
        let admin = admin.to_lowercase();
        let added_by = added_by.to_lowercase();
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO scene_admin (place_id, admin, added_by, active) \
             VALUES ($1, $2, $3, TRUE) \
             RETURNING id",
        )
        .bind(place_id)
        .bind(&admin)
        .bind(&added_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn remove(&self, place_id: &str, admin: &str) -> Result<u64, ApiError> {
        let admin = admin.to_lowercase();
        let res = sqlx::query(
            "UPDATE scene_admin SET active = FALSE WHERE place_id = $1 AND admin = $2 AND active = TRUE",
        )
        .bind(place_id)
        .bind(&admin)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn is_admin(&self, place_id: &str, admin: &str) -> Result<bool, ApiError> {
        let admin = admin.to_lowercase();
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM scene_admin WHERE place_id = $1 AND admin = $2 AND active = TRUE",
        )
        .bind(place_id)
        .bind(&admin)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(n > 0)
    }
}
