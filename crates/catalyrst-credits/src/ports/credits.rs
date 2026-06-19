use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::http::ApiError;

#[derive(Clone)]
pub struct CreditsComponent {
    pub pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct SeasonRow {
    pub id: i32,
    pub name: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub max_mana: f64,
    pub amount_of_weeks: i32,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct WeekRow {
    pub week_number: i32,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UserCreditsRow {
    pub available: f64,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_blocked_for_claiming: bool,
}

#[derive(Debug, Clone)]
pub struct ClaimOutcome {
    pub ok: bool,
    pub credits_granted: f64,
    pub is_blocked_for_claiming: bool,
}

#[derive(Debug, Clone)]
pub struct GoalRow {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub thumbnail: String,
    pub reward: f64,
    pub total_steps: i32,
    pub completed_steps: i32,
    pub is_claimed: bool,
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
        let row = sqlx::query(
            "SELECT has_started_program FROM user_program WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<bool, _>("has_started_program")).unwrap_or(false))
    }

    pub async fn user_credits(&self, address: &str) -> Result<Option<UserCreditsRow>, ApiError> {
        let row = sqlx::query(
            "SELECT available::float8 AS available, expires_at, is_blocked_for_claiming \
             FROM user_credits WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| UserCreditsRow {
            available: r.get::<f64, _>("available"),
            expires_at: r.get("expires_at"),
            is_blocked_for_claiming: r.get("is_blocked_for_claiming"),
        }))
    }

    pub async fn current_week_goals(
        &self,
        address: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<GoalRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT g.id, g.title, g.description, g.thumbnail, g.reward::float8 AS reward, \
                    g.total_steps, \
                    COALESCE(p.completed_steps, 0) AS completed_steps, \
                    COALESCE(p.is_claimed, FALSE) AS is_claimed \
             FROM credits_goals g \
             JOIN credits_weeks w ON w.id = g.week_id \
             LEFT JOIN user_goal_progress p ON p.goal_id = g.id AND p.address = $1 \
             WHERE w.start_date <= $2 AND w.end_date >= $2 \
             ORDER BY g.sort_order, g.id",
        )
        .bind(address)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| GoalRow {
                id: r.get("id"),
                title: r.get("title"),
                description: r.get("description"),
                thumbnail: r.get("thumbnail"),
                reward: r.get::<f64, _>("reward"),
                total_steps: r.get("total_steps"),
                completed_steps: r.get("completed_steps"),
                is_claimed: r.get("is_claimed"),
            })
            .collect())
    }

    pub async fn current_season(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<SeasonRow>, ApiError> {
        let row = sqlx::query(
            "SELECT id, name, start_date, end_date, max_mana::float8 AS max_mana, \
                    amount_of_weeks, state \
             FROM credits_seasons \
             WHERE start_date <= $1 AND end_date >= $1 \
             ORDER BY start_date DESC LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(map_season))
    }

    pub async fn last_season(&self, now: DateTime<Utc>) -> Result<Option<SeasonRow>, ApiError> {
        let row = sqlx::query(
            "SELECT id, name, start_date, end_date, max_mana::float8 AS max_mana, \
                    amount_of_weeks, state \
             FROM credits_seasons \
             WHERE end_date < $1 \
             ORDER BY end_date DESC LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(map_season))
    }

    pub async fn next_season(&self, now: DateTime<Utc>) -> Result<Option<SeasonRow>, ApiError> {
        let row = sqlx::query(
            "SELECT id, name, start_date, end_date, max_mana::float8 AS max_mana, \
                    amount_of_weeks, state \
             FROM credits_seasons \
             WHERE start_date > $1 \
             ORDER BY start_date ASC LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(map_season))
    }

    pub async fn current_week(
        &self,
        season_id: i32,
        now: DateTime<Utc>,
    ) -> Result<Option<WeekRow>, ApiError> {
        let row = sqlx::query(
            "SELECT week_number, start_date, end_date \
             FROM credits_weeks \
             WHERE season_id = $1 AND start_date <= $2 AND end_date >= $2 \
             ORDER BY week_number DESC LIMIT 1",
        )
        .bind(season_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| WeekRow {
            week_number: r.get("week_number"),
            start_date: r.get("start_date"),
            end_date: r.get("end_date"),
        }))
    }

    pub async fn claim_credits(&self, address: &str) -> Result<ClaimOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;

        // Read the balance as exact NUMERIC text: MANA wei exceeds f64's 2^53
        // range and would silently drift. Convert to f64 only at the JSON edge.
        let row = sqlx::query(
            "SELECT available::text AS available, available > 0 AS positive, \
                    is_blocked_for_claiming \
             FROM user_credits WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .fetch_optional(&mut *tx)
        .await?;

        let (available_str, positive, blocked) = match row {
            Some(r) => (
                r.get::<String, _>("available"),
                r.get::<bool, _>("positive"),
                r.get::<bool, _>("is_blocked_for_claiming"),
            ),
            None => ("0".to_string(), false, false),
        };

        if blocked {
            tx.rollback().await?;
            return Ok(ClaimOutcome {
                ok: false,
                credits_granted: 0.0,
                is_blocked_for_claiming: true,
            });
        }

        if !positive {
            tx.commit().await?;
            return Ok(ClaimOutcome {
                ok: true,
                credits_granted: 0.0,
                is_blocked_for_claiming: false,
            });
        }

        // Zero the balance and record the pre-zeroing value into the ledger atomically.
        sqlx::query(
            "WITH moved AS ( \
                 UPDATE user_credits \
                 SET available = 0, updated_at = now() \
                 WHERE address = $1 \
                 RETURNING $2::numeric AS amount \
             ) \
             INSERT INTO credit_ledger (address, kind, amount, captcha_ok) \
             SELECT $1, 'claim', amount, TRUE FROM moved",
        )
        .bind(address)
        .bind(&available_str)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let credits_granted = available_str.parse::<f64>().unwrap_or(0.0);

        Ok(ClaimOutcome {
            ok: true,
            credits_granted,
            is_blocked_for_claiming: false,
        })
    }
}

fn map_season(r: sqlx::postgres::PgRow) -> SeasonRow {
    SeasonRow {
        id: r.get("id"),
        name: r.get("name"),
        start_date: r.get("start_date"),
        end_date: r.get("end_date"),
        max_mana: r.get::<f64, _>("max_mana"),
        amount_of_weeks: r.get("amount_of_weeks"),
        state: r.get("state"),
    }
}
