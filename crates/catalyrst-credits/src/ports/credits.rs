use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use sqlx::postgres::PgPool;
use sqlx::Row;
use tokio::sync::Notify;

use crate::http::ApiError;
use crate::ports::packs::PackRow;

/// Active packs change through the admin API only; public reads share one row set for this long.
pub const PACKS_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct CreditsComponent {
    pub pool: PgPool,
    pub(crate) packs: Arc<TtlMap<(), Vec<PackRow>>>,
    pub(crate) kick: Arc<Notify>,
}

#[derive(Debug, Clone)]
pub struct UserCreditsRow {
    pub available: f64,
    pub earned_available: f64,
    pub is_blocked_for_claiming: bool,
}

#[derive(Debug, Clone)]
pub struct ClaimOutcome {
    pub ok: bool,
    pub credits_granted: f64,
    pub is_blocked_for_claiming: bool,
}

impl CreditsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            packs: Arc::new(TtlMap::new("credits-packs", PACKS_CACHE_TTL)),
            kick: Arc::new(Notify::new()),
        }
    }

    /// Wake the fulfilment worker ahead of its next tick.
    pub(crate) fn kick_fulfillment(&self) {
        self.kick.notify_one();
    }

    pub async fn mark_started(&self, address: &str) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO user_program (address, has_started_program) \
             VALUES ($1, TRUE) \
             ON CONFLICT (address) DO UPDATE SET has_started_program = TRUE",
        )
        .bind(address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn has_started(&self, address: &str) -> Result<bool, ApiError> {
        let row = sqlx::query("SELECT has_started_program FROM user_program WHERE address = $1")
            .bind(address)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row
            .map(|r| r.get::<bool, _>("has_started_program"))
            .unwrap_or(false))
    }

    pub async fn user_credits(&self, address: &str) -> Result<Option<UserCreditsRow>, ApiError> {
        let row = sqlx::query(
            "SELECT available::float8 AS available, \
                    earned_available::float8 AS earned_available, \
                    is_blocked_for_claiming \
             FROM user_credits WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| UserCreditsRow {
            available: r.get::<f64, _>("available"),
            earned_available: r.get::<f64, _>("earned_available"),
            is_blocked_for_claiming: r.get("is_blocked_for_claiming"),
        }))
    }

    pub async fn user_progress(
        &self,
        address: &str,
    ) -> Result<(bool, Option<UserCreditsRow>), ApiError> {
        let row = sqlx::query(
            "SELECT COALESCE(p.has_started_program, false) AS started,
                    c.address IS NOT NULL AS has_credits,
                    c.available::float8 AS available,
                    c.earned_available::float8 AS earned_available,
                    c.is_blocked_for_claiming
             FROM (VALUES ($1::text)) AS requested(address)
             LEFT JOIN user_program p USING (address)
             LEFT JOIN user_credits c USING (address)",
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;
        let credits = row.get::<bool, _>("has_credits").then(|| UserCreditsRow {
            available: row.get("available"),
            earned_available: row.get("earned_available"),
            is_blocked_for_claiming: row.get("is_blocked_for_claiming"),
        });
        Ok((row.get("started"), credits))
    }

    pub async fn claim_credits(&self, address: &str) -> Result<ClaimOutcome, ApiError> {
        let blocked =
            sqlx::query("SELECT is_blocked_for_claiming FROM user_credits WHERE address = $1")
                .bind(address)
                .fetch_optional(&self.pool)
                .await?
                .map(|r| r.get::<bool, _>("is_blocked_for_claiming"))
                .unwrap_or(false);

        Ok(ClaimOutcome {
            ok: !blocked,
            credits_granted: 0.0,
            is_blocked_for_claiming: blocked,
        })
    }
}
