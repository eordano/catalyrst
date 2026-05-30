use std::collections::HashSet;

use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize)]
pub struct DenylistEntry {
    pub entity_id: String,
    pub reason: Option<String>,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Default)]
pub struct RegistryStore {
    pool: Option<PgPool>,
}

impl RegistryStore {
    pub fn new(pool: Option<PgPool>) -> Self {
        Self { pool }
    }

    pub fn enabled(&self) -> bool {
        self.pool.is_some()
    }

    pub async fn denylist_rows(&self) -> Result<Vec<DenylistEntry>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT entity_id, reason, created_by, created_at, updated_at \
             FROM denylist ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(row_to_entry).collect())
    }

    pub async fn denylist_set(&self) -> Result<HashSet<String>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(HashSet::new());
        };
        let rows = sqlx::query("SELECT entity_id FROM denylist")
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("entity_id"))
            .collect())
    }

    pub async fn add_to_denylist(
        &self,
        entity_id: &str,
        created_by: &str,
        reason: Option<&str>,
    ) -> Result<DenylistEntry, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Err(disabled());
        };
        let now = now_ms();
        let row = sqlx::query(
            "INSERT INTO denylist (entity_id, reason, created_by, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $4) \
             ON CONFLICT (entity_id) DO UPDATE \
               SET reason = EXCLUDED.reason, updated_at = EXCLUDED.updated_at \
             RETURNING entity_id, reason, created_by, created_at, updated_at",
        )
        .bind(entity_id.to_lowercase())
        .bind(reason)
        .bind(created_by.to_lowercase())
        .bind(now)
        .fetch_one(pool)
        .await?;
        Ok(row_to_entry(row))
    }

    pub async fn remove_from_denylist(&self, entity_id: &str) -> Result<bool, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Err(disabled());
        };
        let res = sqlx::query("DELETE FROM denylist WHERE lower(entity_id) = $1")
            .bind(entity_id.to_lowercase())
            .execute(pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn world_spawn(
        &self,
        world_name: &str,
    ) -> Result<Option<(i64, i64)>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT x, y FROM world_spawn_coordinates WHERE world_name = $1 AND is_user_set = true",
        )
        .bind(world_name)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| (r.get::<i64, _>("x"), r.get::<i64, _>("y"))))
    }
}

fn disabled() -> sqlx::Error {
    sqlx::Error::Configuration(
        "ab_registry DB not configured (set AB_REGISTRY_PG_CONNECTION_STRING)".into(),
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn row_to_entry(r: sqlx::postgres::PgRow) -> DenylistEntry {
    DenylistEntry {
        entity_id: r.get("entity_id"),
        reason: r.try_get::<Option<String>, _>("reason").unwrap_or(None),
        created_by: r.try_get::<Option<String>, _>("created_by").unwrap_or(None),
        created_at: r.try_get("created_at").unwrap_or(0),
        updated_at: r.try_get("updated_at").unwrap_or(0),
    }
}
