use sqlx::PgPool;
use uuid::Uuid;

use crate::http::ApiError;

/// One active scene-admin row. Mirrors upstream `SceneAdmin`
/// (comms-gatekeeper `src/types.ts`): `created_at` is epoch milliseconds (the
/// upstream column is `bigint`), serialized as a JSON number so the wire object
/// shape is `SceneAdmin & {name, canBeRemoved}`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SceneAdminRow {
    pub id: Uuid,
    pub place_id: String,
    pub admin: String,
    pub added_by: String,
    /// epoch milliseconds
    pub created_at: i64,
    pub active: bool,
}

pub struct SceneAdminComponent {
    pool: PgPool,
}

impl SceneAdminComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Active admins for a place, optionally filtered by a single admin address.
    /// Ports `sceneAdminManager.listActiveAdmins` (comms-gatekeeper
    /// `scene-admin-manager.ts`): `SELECT * FROM scene_admin WHERE active = true
    /// AND place_id = $1 [AND admin = $2]`, with `admin` lowercased.
    pub async fn list_active_admins(
        &self,
        place_id: &str,
        admin: Option<&str>,
    ) -> Result<Vec<SceneAdminRow>, ApiError> {
        // `created_at` is stored as TIMESTAMP here but upstream stores bigint
        // epoch-ms; convert so the emitted JSON value matches upstream's number.
        let mut sql = String::from(
            "SELECT id, place_id, admin, added_by, \
             (EXTRACT(EPOCH FROM created_at) * 1000)::int8 AS created_at, active \
             FROM scene_admin WHERE active = TRUE AND place_id = $1",
        );
        if admin.is_some() {
            sql.push_str(" AND admin = $2");
        }
        let mut q = sqlx::query_as::<_, SceneAdminRow>(sqlx::AssertSqlSafe(sql)).bind(place_id);
        if let Some(a) = admin {
            q = q.bind(a.to_lowercase());
        }
        Ok(q.fetch_all(&self.pool).await?)
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
