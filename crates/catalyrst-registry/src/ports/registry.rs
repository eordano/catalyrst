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

    /// Returns whether the build queue is operator-paused. Defaults to false
    /// when the registry DB is unconfigured (no control plane available).
    pub async fn queue_paused(&self) -> Result<bool, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(false);
        };
        let row = sqlx::query("SELECT paused FROM queue_control WHERE id = 1")
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|r| r.get::<bool, _>("paused")).unwrap_or(false))
    }

    /// Sets the operator queue-pause flag, returning the new value.
    pub async fn set_queue_paused(
        &self,
        paused: bool,
        updated_by: &str,
    ) -> Result<bool, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Err(disabled());
        };
        let now = now_ms();
        sqlx::query(
            "INSERT INTO queue_control (id, paused, updated_by, updated_at) \
             VALUES (1, $1, $2, $3) \
             ON CONFLICT (id) DO UPDATE \
               SET paused = EXCLUDED.paused, \
                   updated_by = EXCLUDED.updated_by, \
                   updated_at = EXCLUDED.updated_at",
        )
        .bind(paused)
        .bind(updated_by)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(paused)
    }

    /// Records (or bumps) a retry request for an entity, returning the attempt
    /// count after the upsert.
    pub async fn record_retry(
        &self,
        entity_id: &str,
        requested_by: &str,
    ) -> Result<i32, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Err(disabled());
        };
        let now = now_ms();
        let row = sqlx::query(
            "INSERT INTO build_retries (entity_id, requested_by, requested_at, attempts) \
             VALUES ($1, $2, $3, 1) \
             ON CONFLICT (entity_id) DO UPDATE \
               SET requested_by = EXCLUDED.requested_by, \
                   requested_at = EXCLUDED.requested_at, \
                   attempts = build_retries.attempts + 1 \
             RETURNING attempts",
        )
        .bind(entity_id.to_lowercase())
        .bind(requested_by)
        .bind(now)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<i32, _>("attempts"))
    }

    /// The platforms a build job is tracked for. Mirrors the platforms
    /// `/queues/status` reports (windows, mac, webgl, linux).
    pub const BUILD_PLATFORMS: [&'static str; 4] = ["windows", "mac", "webgl", "linux"];

    /// Re-enqueue an entity for building: reset every platform's build status to
    /// `pending` in the catalyrst-owned build-job queue so a worker picks it up
    /// and `/queues/status` reports it as pending. Returns the platforms that
    /// were (re)enqueued. Idempotent — an existing row is reset to `pending`.
    pub async fn enqueue_build(
        &self,
        entity_id: &str,
        requested_by: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Err(disabled());
        };
        let now = now_ms();
        let id = entity_id.to_lowercase();
        let mut enqueued = Vec::with_capacity(Self::BUILD_PLATFORMS.len());
        for platform in Self::BUILD_PLATFORMS {
            sqlx::query(
                "INSERT INTO build_jobs (entity_id, platform, status, requested_by, enqueued_at, updated_at) \
                 VALUES ($1, $2, 'pending', $3, $4, $4) \
                 ON CONFLICT (entity_id, platform) DO UPDATE \
                   SET status = 'pending', \
                       requested_by = EXCLUDED.requested_by, \
                       enqueued_at = EXCLUDED.enqueued_at, \
                       updated_at = EXCLUDED.updated_at",
            )
            .bind(&id)
            .bind(platform)
            .bind(requested_by)
            .bind(now)
            .execute(pool)
            .await?;
            enqueued.push(platform.to_string());
        }
        Ok(enqueued)
    }

    /// All entity ids with a `pending` build job for the given platform. Used by
    /// `/queues/status` to union DB-enqueued pending jobs with the disk-derived
    /// set. Returns an empty set when the registry DB is unconfigured.
    pub async fn pending_jobs_for(&self, platform: &str) -> Result<HashSet<String>, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(HashSet::new());
        };
        let rows = sqlx::query(
            "SELECT entity_id FROM build_jobs WHERE status = 'pending' AND platform = $1",
        )
        .bind(platform)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("entity_id"))
            .collect())
    }

    pub async fn world_spawn(&self, world_name: &str) -> Result<Option<(i64, i64)>, sqlx::Error> {
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
