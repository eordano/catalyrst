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
    pub earned_available: f64,
    pub earned_expires_at: Option<DateTime<Utc>>,
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
                    earned_expires_at, is_blocked_for_claiming \
             FROM user_credits WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| UserCreditsRow {
            available: r.get::<f64, _>("available"),
            earned_available: r.get::<f64, _>("earned_available"),
            earned_expires_at: r.get("earned_expires_at"),
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

    pub async fn current_season(&self, now: DateTime<Utc>) -> Result<Option<SeasonRow>, ApiError> {
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

    pub async fn claim_credits(
        &self,
        address: &str,
        now: DateTime<Utc>,
    ) -> Result<ClaimOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;

        let blocked = sqlx::query(
            "SELECT is_blocked_for_claiming \
             FROM user_credits WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .fetch_optional(&mut *tx)
        .await?
        .map(|r| r.get::<bool, _>("is_blocked_for_claiming"))
        .unwrap_or(false);

        if blocked {
            tx.rollback().await?;
            return Ok(ClaimOutcome {
                ok: false,
                credits_granted: 0.0,
                is_blocked_for_claiming: true,
            });
        }

        let claimable = sqlx::query(
            "SELECT p.goal_id, g.reward::text AS reward \
             FROM user_goal_progress p \
             JOIN credits_goals g ON g.id = p.goal_id \
             JOIN credits_weeks w ON w.id = g.week_id \
             WHERE p.address = $1 AND p.is_claimed = FALSE \
               AND p.completed_steps >= g.total_steps \
               AND w.start_date <= $2 AND w.end_date >= $2 \
             FOR UPDATE OF p",
        )
        .bind(address)
        .bind(now)
        .fetch_all(&mut *tx)
        .await?;

        let goal_ids: Vec<i32> = claimable.iter().map(|r| r.get("goal_id")).collect();
        if goal_ids.is_empty() {
            tx.commit().await?;
            return Ok(ClaimOutcome {
                ok: true,
                credits_granted: 0.0,
                is_blocked_for_claiming: false,
            });
        }

        let marked = sqlx::query(
            "UPDATE user_goal_progress SET is_claimed = TRUE \
             WHERE address = $1 AND goal_id = ANY($2) AND is_claimed = FALSE \
             RETURNING goal_id",
        )
        .bind(address)
        .bind(&goal_ids)
        .fetch_all(&mut *tx)
        .await?;
        let marked_ids: std::collections::HashSet<i32> =
            marked.iter().map(|r| r.get::<i32, _>("goal_id")).collect();
        if marked_ids.is_empty() {
            tx.commit().await?;
            return Ok(ClaimOutcome {
                ok: true,
                credits_granted: 0.0,
                is_blocked_for_claiming: false,
            });
        }

        let claimed_ids: Vec<i32> = marked_ids.iter().copied().collect();
        let total: String = sqlx::query(
            "SELECT COALESCE(SUM(reward), 0)::text AS total \
             FROM credits_goals WHERE id = ANY($1)",
        )
        .bind(&claimed_ids)
        .fetch_one(&mut *tx)
        .await?
        .get("total");

        self.expire_earned_in_tx(&mut tx, address).await?;

        let season_end: DateTime<Utc> = sqlx::query(
            "SELECT s.end_date FROM credits_seasons s \
             JOIN credits_weeks w ON w.season_id = s.id \
             JOIN credits_goals g ON g.week_id = w.id \
             WHERE g.id = ANY($1) ORDER BY s.end_date DESC LIMIT 1",
        )
        .bind(&claimed_ids)
        .fetch_one(&mut *tx)
        .await?
        .get("end_date");

        sqlx::query(
            "INSERT INTO user_credits (address, available, earned_available, earned_expires_at, updated_at) \
             VALUES ($1, $2::numeric, $2::numeric, $3, now()) \
             ON CONFLICT (address) DO UPDATE \
             SET available = user_credits.available + EXCLUDED.available, \
                 earned_available = user_credits.earned_available + EXCLUDED.earned_available, \
                 earned_expires_at = GREATEST(COALESCE(user_credits.earned_expires_at, EXCLUDED.earned_expires_at), \
                                              EXCLUDED.earned_expires_at), \
                 updated_at = now()",
        )
        .bind(address)
        .bind(&total)
        .bind(season_end)
        .execute(&mut *tx)
        .await?;

        let mut tx_ref_ids: Vec<String> = claimed_ids.iter().map(|id| id.to_string()).collect();
        tx_ref_ids.sort();
        let tx_ref = format!("goals:{}", tx_ref_ids.join("+"));
        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
             VALUES ($1, 'claim', $2::numeric, $3, 'earned', TRUE)",
        )
        .bind(address)
        .bind(&total)
        .bind(&tx_ref)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO user_program (address, has_started_program) \
             VALUES ($1, TRUE) ON CONFLICT (address) DO NOTHING",
        )
        .bind(address)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(ClaimOutcome {
            ok: true,
            credits_granted: total.parse::<f64>().unwrap_or(0.0),
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
