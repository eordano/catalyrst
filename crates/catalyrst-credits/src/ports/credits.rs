use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::http::ApiError;

#[derive(Clone)]
pub struct CreditsComponent {
    pub pool: PgPool,
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
        Self { pool }
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

    // Seasons, weeks, and goals were removed (legacy); there is nothing left to
    // claim, but POST /captcha stays wire-compatible: the slider gate still runs
    // and the response keeps its shape with zero credits granted.
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
