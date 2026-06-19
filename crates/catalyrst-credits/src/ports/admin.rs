//! Admin-only data access for the financial controls. Each mutation appends a
//! `credit_ledger` and `admin_audit` row in one transaction. Amounts are exact
//! NUMERIC text, never f64 (MANA wei exceeds f64's 2^53 range); revoke is clamped
//! to 0 and records only the amount actually removed.

use serde_json::Value as JsonValue;
use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;

#[derive(Debug, Clone)]
pub struct SeasonAdminRow {
    pub id: i32,
    pub name: String,
    pub start_date: chrono::DateTime<chrono::Utc>,
    pub end_date: chrono::DateTime<chrono::Utc>,
    pub max_mana: String,
    pub amount_of_weeks: i32,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct GoalAdminRow {
    pub id: i32,
    pub week_id: i32,
    pub title: String,
    pub description: String,
    pub thumbnail: String,
    pub reward: String,
    pub total_steps: i32,
    pub sort_order: i32,
}

#[derive(Debug, Clone)]
pub struct GrantOutcome {
    /// Resulting balance, NUMERIC text.
    pub available: String,
    /// Amount actually applied (clamped for revoke), NUMERIC text.
    pub applied: String,
    /// True when this was a no-op replay of a prior idempotency key.
    pub replayed: bool,
}

impl CreditsComponent {
    #[allow(clippy::too_many_arguments)]
    async fn audit<'e, E>(
        executor: E,
        action: &str,
        address: Option<&str>,
        entity_id: Option<i64>,
        amount: Option<&str>,
        reason: Option<&str>,
        actor: Option<&str>,
        detail: &JsonValue,
    ) -> Result<(), ApiError>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query(
            "INSERT INTO admin_audit (action, address, entity_id, amount, reason, actor, detail) \
             VALUES ($1, $2, $3, $4::numeric, $5, $6, $7)",
        )
        .bind(action)
        .bind(address)
        .bind(entity_id)
        .bind(amount)
        .bind(reason)
        .bind(actor)
        .bind(detail)
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn admin_list_seasons(&self) -> Result<Vec<SeasonAdminRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, name, start_date, end_date, max_mana::text AS max_mana, \
                    amount_of_weeks, state \
             FROM credits_seasons ORDER BY start_date DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(map_season_admin).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_create_season(
        &self,
        name: &str,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        max_mana: &str,
        amount_of_weeks: i32,
        state: &str,
        detail: &JsonValue,
    ) -> Result<SeasonAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "INSERT INTO credits_seasons \
                 (name, start_date, end_date, max_mana, amount_of_weeks, state) \
             VALUES ($1, $2, $3, $4::numeric, $5, $6) \
             RETURNING id, name, start_date, end_date, max_mana::text AS max_mana, \
                       amount_of_weeks, state",
        )
        .bind(name)
        .bind(start_date)
        .bind(end_date)
        .bind(max_mana)
        .bind(amount_of_weeks)
        .bind(state)
        .fetch_one(&mut *tx)
        .await?;
        let season = map_season_admin(row);
        Self::audit(
            &mut *tx,
            "season.create",
            None,
            Some(season.id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(season)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_update_season(
        &self,
        id: i32,
        name: &str,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        max_mana: &str,
        amount_of_weeks: i32,
        state: &str,
        detail: &JsonValue,
    ) -> Result<SeasonAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE credits_seasons SET \
                 name = $2, start_date = $3, end_date = $4, max_mana = $5::numeric, \
                 amount_of_weeks = $6, state = $7 \
             WHERE id = $1 \
             RETURNING id, name, start_date, end_date, max_mana::text AS max_mana, \
                       amount_of_weeks, state",
        )
        .bind(id)
        .bind(name)
        .bind(start_date)
        .bind(end_date)
        .bind(max_mana)
        .bind(amount_of_weeks)
        .bind(state)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or_else(|| ApiError::not_found("season not found"))?;
        let season = map_season_admin(row);
        Self::audit(
            &mut *tx,
            "season.update",
            None,
            Some(season.id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(season)
    }

    pub async fn admin_delete_season(
        &self,
        id: i32,
        detail: &JsonValue,
    ) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query("DELETE FROM credits_seasons WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(ApiError::not_found("season not found"));
        }
        Self::audit(
            &mut *tx,
            "season.delete",
            None,
            Some(id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn admin_list_goals(
        &self,
        week_id: Option<i32>,
    ) -> Result<Vec<GoalAdminRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, week_id, title, description, thumbnail, reward::text AS reward, \
                    total_steps, sort_order \
             FROM credits_goals \
             WHERE ($1::int IS NULL OR week_id = $1) \
             ORDER BY week_id, sort_order, id",
        )
        .bind(week_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(map_goal_admin).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_create_goal(
        &self,
        week_id: i32,
        title: &str,
        description: &str,
        thumbnail: &str,
        reward: &str,
        total_steps: i32,
        sort_order: i32,
        detail: &JsonValue,
    ) -> Result<GoalAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        // Validate FK explicitly so a bad week_id is a 404, not a 500.
        let week_exists =
            sqlx::query("SELECT 1 FROM credits_weeks WHERE id = $1")
                .bind(week_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
        if !week_exists {
            tx.rollback().await?;
            return Err(ApiError::not_found("week not found"));
        }
        let row = sqlx::query(
            "INSERT INTO credits_goals \
                 (week_id, title, description, thumbnail, reward, total_steps, sort_order) \
             VALUES ($1, $2, $3, $4, $5::numeric, $6, $7) \
             RETURNING id, week_id, title, description, thumbnail, reward::text AS reward, \
                       total_steps, sort_order",
        )
        .bind(week_id)
        .bind(title)
        .bind(description)
        .bind(thumbnail)
        .bind(reward)
        .bind(total_steps)
        .bind(sort_order)
        .fetch_one(&mut *tx)
        .await?;
        let goal = map_goal_admin(row);
        Self::audit(
            &mut *tx,
            "goal.create",
            None,
            Some(goal.id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(goal)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_update_goal(
        &self,
        id: i32,
        title: &str,
        description: &str,
        thumbnail: &str,
        reward: &str,
        total_steps: i32,
        sort_order: i32,
        detail: &JsonValue,
    ) -> Result<GoalAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE credits_goals SET \
                 title = $2, description = $3, thumbnail = $4, reward = $5::numeric, \
                 total_steps = $6, sort_order = $7 \
             WHERE id = $1 \
             RETURNING id, week_id, title, description, thumbnail, reward::text AS reward, \
                       total_steps, sort_order",
        )
        .bind(id)
        .bind(title)
        .bind(description)
        .bind(thumbnail)
        .bind(reward)
        .bind(total_steps)
        .bind(sort_order)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or_else(|| ApiError::not_found("goal not found"))?;
        let goal = map_goal_admin(row);
        Self::audit(
            &mut *tx,
            "goal.update",
            None,
            Some(goal.id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(goal)
    }

    pub async fn admin_delete_goal(
        &self,
        id: i32,
        detail: &JsonValue,
    ) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query("DELETE FROM credits_goals WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(ApiError::not_found("goal not found"));
        }
        Self::audit(
            &mut *tx,
            "goal.delete",
            None,
            Some(id as i64),
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Grant credits to a wallet (`amount` is NUMERIC text, > 0 per the caller).
    ///
    /// With an `idempotency_key` the grant is safe to retry: the de-dup claims the
    /// key with `INSERT ... ON CONFLICT DO NOTHING` inside the grant transaction,
    /// so concurrent retries can never double-grant (the key's UNIQUE PK serializes
    /// them and the loser reads the committed prior result).
    pub async fn admin_grant_credits(
        &self,
        address: &str,
        amount: &str,
        reason: Option<&str>,
        actor: Option<&str>,
        idempotency_key: Option<&str>,
        detail: &JsonValue,
    ) -> Result<GrantOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;

        // Reserve the key; a zero-row insert means a prior grant already used it.
        if let Some(key) = idempotency_key {
            let claimed = sqlx::query(
                "INSERT INTO credit_grant_idempotency \
                     (idempotency_key, address, amount, available, actor) \
                 VALUES ($1, $2, $3::numeric, 0, $4) \
                 ON CONFLICT (idempotency_key) DO NOTHING \
                 RETURNING idempotency_key",
            )
            .bind(key)
            .bind(address)
            .bind(amount)
            .bind(actor)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();

            if !claimed {
                // Replay. Confirm address+amount match the original before
                // returning its result, else a key reused for a different grant
                // would leak that balance and silently no-op this grant: reject 409.
                let prior = sqlx::query(
                    "SELECT available::text AS available, amount::text AS amount, \
                            (lower(address) = lower($2)) AS addr_match, \
                            (amount = $3::numeric) AS amount_match \
                     FROM credit_grant_idempotency WHERE idempotency_key = $1",
                )
                .bind(key)
                .bind(address)
                .bind(amount)
                .fetch_one(&mut *tx)
                .await?;
                let addr_match: bool = prior.get("addr_match");
                let amount_match: bool = prior.get("amount_match");
                if !addr_match || !amount_match {
                    return Err(ApiError::conflict(
                        "idempotency key already used for a different grant (address/amount mismatch)",
                    ));
                }
                tx.commit().await?;
                return Ok(GrantOutcome {
                    available: prior.get("available"),
                    applied: prior.get("amount"),
                    replayed: true,
                });
            }
        }

        let row = sqlx::query(
            "INSERT INTO user_credits (address, available, updated_at) \
             VALUES ($1, $2::numeric, now()) \
             ON CONFLICT (address) DO UPDATE \
                 SET available = user_credits.available + $2::numeric, updated_at = now() \
             RETURNING available::text AS available",
        )
        .bind(address)
        .bind(amount)
        .fetch_one(&mut *tx)
        .await?;
        let available: String = row.get("available");

        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, captcha_ok) \
             VALUES ($1, 'grant', $2::numeric, FALSE)",
        )
        .bind(address)
        .bind(amount)
        .execute(&mut *tx)
        .await?;

        // Persist the resulting balance so a replay returns the same result.
        if let Some(key) = idempotency_key {
            sqlx::query(
                "UPDATE credit_grant_idempotency \
                 SET available = $2::numeric WHERE idempotency_key = $1",
            )
            .bind(key)
            .bind(&available)
            .execute(&mut *tx)
            .await?;
        }

        Self::audit(
            &mut *tx,
            "credits.grant",
            Some(address),
            None,
            Some(amount),
            reason,
            actor,
            detail,
        )
        .await?;
        tx.commit().await?;

        Ok(GrantOutcome {
            available,
            applied: amount.to_string(),
            replayed: false,
        })
    }

    /// Revoke up to `amount` from a wallet (`amount` is NUMERIC text, > 0 per the
    /// caller). Never driven negative: the removed amount is clamped to the
    /// balance and that exact value is what the ledger records.
    pub async fn admin_revoke_credits(
        &self,
        address: &str,
        amount: &str,
        reason: Option<&str>,
        actor: Option<&str>,
        detail: &JsonValue,
    ) -> Result<GrantOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;

        let current = sqlx::query(
            "SELECT available::text AS available FROM user_credits \
             WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(current) = current else {
            tx.rollback().await?;
            return Err(ApiError::not_found("user has no credits balance"));
        };
        let available_before: String = current.get("available");

        // removed = available - GREATEST(available - amount, 0), all in NUMERIC.
        let row = sqlx::query(
            "UPDATE user_credits \
             SET available = GREATEST(available - $2::numeric, 0), updated_at = now() \
             WHERE address = $1 \
             RETURNING available::text AS available, \
                       ($3::numeric - GREATEST($3::numeric - $2::numeric, 0))::text AS removed",
        )
        .bind(address)
        .bind(amount)
        .bind(&available_before)
        .fetch_one(&mut *tx)
        .await?;
        let available_after: String = row.get("available");
        let removed: String = row.get("removed");

        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, captcha_ok) \
             VALUES ($1, 'consume', $2::numeric, FALSE)",
        )
        .bind(address)
        .bind(&removed)
        .execute(&mut *tx)
        .await?;

        let mut detail = detail.clone();
        if let JsonValue::Object(map) = &mut detail {
            map.insert("requested".into(), JsonValue::String(amount.to_string()));
            map.insert("removed".into(), JsonValue::String(removed.clone()));
        }

        Self::audit(
            &mut *tx,
            "credits.revoke",
            Some(address),
            None,
            Some(&removed),
            reason,
            actor,
            &detail,
        )
        .await?;
        tx.commit().await?;

        Ok(GrantOutcome {
            available: available_after,
            applied: removed,
            replayed: false,
        })
    }

    /// Set `is_blocked_for_claiming` for a wallet, creating the row if absent so
    /// a block always takes effect.
    pub async fn admin_set_blocked(
        &self,
        address: &str,
        blocked: bool,
        reason: Option<&str>,
        actor: Option<&str>,
        detail: &JsonValue,
    ) -> Result<bool, ApiError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO user_credits (address, is_blocked_for_claiming, updated_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (address) DO UPDATE \
                 SET is_blocked_for_claiming = $2, updated_at = now()",
        )
        .bind(address)
        .bind(blocked)
        .execute(&mut *tx)
        .await?;

        Self::audit(
            &mut *tx,
            if blocked { "user.block" } else { "user.unblock" },
            Some(address),
            None,
            None,
            reason,
            actor,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(blocked)
    }
}

fn map_season_admin(r: sqlx::postgres::PgRow) -> SeasonAdminRow {
    SeasonAdminRow {
        id: r.get("id"),
        name: r.get("name"),
        start_date: r.get("start_date"),
        end_date: r.get("end_date"),
        max_mana: r.get("max_mana"),
        amount_of_weeks: r.get("amount_of_weeks"),
        state: r.get("state"),
    }
}

fn map_goal_admin(r: sqlx::postgres::PgRow) -> GoalAdminRow {
    GoalAdminRow {
        id: r.get("id"),
        week_id: r.get("week_id"),
        title: r.get("title"),
        description: r.get("description"),
        thumbnail: r.get("thumbnail"),
        reward: r.get("reward"),
        total_steps: r.get("total_steps"),
        sort_order: r.get("sort_order"),
    }
}
