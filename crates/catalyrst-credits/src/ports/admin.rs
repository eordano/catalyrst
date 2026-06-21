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
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct GrantOutcome {
    pub available: String,

    pub applied: String,

    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct GrantReplayRow {
    pub address: String,
    pub amount: String,
    pub available: String,
}

#[derive(Debug, Clone)]
pub struct PackAdminRow {
    pub sku: String,
    pub title: String,

    pub credits: String,

    pub price_cents: i64,
    pub currency: String,
    pub active: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone)]
pub struct PurchaseAdminRow {
    pub id: i64,
    pub address: String,
    pub sku: String,
    pub credits: String,
    pub amount_cents: i64,
    pub currency: String,
    pub stripe_payment_intent: Option<String>,
    pub method: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct OutboxLineRow {
    pub id: i64,
    pub item_id: String,
    pub urn: String,
    pub token_id: Option<String>,
    pub unit_price_credits: String,
    pub mode: String,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CheckoutAdminRow {
    pub id: i64,
    pub address: String,
    pub total_credits: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub lines: Vec<OutboxLineRow>,
}

#[derive(Debug, Clone)]
pub struct LedgerEntryRow {
    pub id: i64,
    pub address: String,
    pub kind: String,
    pub amount: String,
    pub tx_ref: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct UsageGrantRow {
    pub grantee_address: String,
    pub urn: String,
    pub token_id: Option<String>,
    pub collection: Option<String>,
    pub status: String,
    pub unlock_at: chrono::DateTime<chrono::Utc>,
}

impl CreditsComponent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn audit<'e, E>(
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

    pub async fn admin_delete_season(&self, id: i32, detail: &JsonValue) -> Result<(), ApiError> {
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
                    total_steps, sort_order, kind \
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
        kind: &str,
        detail: &JsonValue,
    ) -> Result<GoalAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;

        let week_exists = sqlx::query("SELECT 1 FROM credits_weeks WHERE id = $1")
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
                 (week_id, title, description, thumbnail, reward, total_steps, sort_order, kind) \
             VALUES ($1, $2, $3, $4, $5::numeric, $6, $7, $8) \
             RETURNING id, week_id, title, description, thumbnail, reward::text AS reward, \
                       total_steps, sort_order, kind",
        )
        .bind(week_id)
        .bind(title)
        .bind(description)
        .bind(thumbnail)
        .bind(reward)
        .bind(total_steps)
        .bind(sort_order)
        .bind(kind)
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
        kind: &str,
        detail: &JsonValue,
    ) -> Result<GoalAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE credits_goals SET \
                 title = $2, description = $3, thumbnail = $4, reward = $5::numeric, \
                 total_steps = $6, sort_order = $7, kind = $8 \
             WHERE id = $1 \
             RETURNING id, week_id, title, description, thumbnail, reward::text AS reward, \
                       total_steps, sort_order, kind",
        )
        .bind(id)
        .bind(title)
        .bind(description)
        .bind(thumbnail)
        .bind(reward)
        .bind(total_steps)
        .bind(sort_order)
        .bind(kind)
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

    pub async fn admin_delete_goal(&self, id: i32, detail: &JsonValue) -> Result<(), ApiError> {
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

    pub async fn find_grant_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<GrantReplayRow>, ApiError> {
        let row = sqlx::query(
            "SELECT address, amount::text AS amount, available::text AS available \
             FROM credit_grant_idempotency WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| GrantReplayRow {
            address: r.get("address"),
            amount: r.get("amount"),
            available: r.get("available"),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_grant_credits(
        &self,
        address: &str,
        amount: &str,
        kind: &str,
        reason: Option<&str>,
        actor: Option<&str>,
        idempotency_key: Option<&str>,
        detail: &JsonValue,
    ) -> Result<GrantOutcome, ApiError> {
        if !matches!(kind, "grant" | "purchase" | "refund") {
            return Err(ApiError::bad_request(
                "admin_grant_credits kind must be one of grant|purchase|refund",
            ));
        }
        let mut tx = self.pool.begin().await?;

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
            "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
             VALUES ($1, $3, $2::numeric, 'paid', FALSE)",
        )
        .bind(address)
        .bind(amount)
        .bind(kind)
        .execute(&mut *tx)
        .await?;

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

        let Some(_) = current else {
            tx.rollback().await?;
            return Err(ApiError::not_found("user has no credits balance"));
        };
        self.expire_earned_in_tx(&mut tx, address).await?;
        let current = sqlx::query(
            "SELECT available::text AS available, earned_available::text AS earned \
             FROM user_credits WHERE address = $1",
        )
        .bind(address)
        .fetch_one(&mut *tx)
        .await?;
        let available_before: String = current.get("available");
        let earned_before: String = current.get("earned");

        let row = sqlx::query(
            "UPDATE user_credits \
             SET available = GREATEST(available - $2::numeric, 0), \
                 earned_available = LEAST(earned_available, GREATEST(available - $2::numeric, 0)), \
                 updated_at = now() \
             WHERE address = $1 \
             RETURNING available::text AS available, \
                       ($3::numeric - GREATEST($3::numeric - $2::numeric, 0))::text AS removed, \
                       ($4::numeric - earned_available)::text AS earned_removed, \
                       GREATEST(($3::numeric - GREATEST($3::numeric - $2::numeric, 0)) \
                                - ($4::numeric - earned_available), 0)::text AS paid_removed",
        )
        .bind(address)
        .bind(amount)
        .bind(&available_before)
        .bind(&earned_before)
        .fetch_one(&mut *tx)
        .await?;
        let available_after: String = row.get("available");
        let removed: String = row.get("removed");
        let earned_removed: String = row.get("earned_removed");
        let paid_removed: String = row.get("paid_removed");

        let mut consume_rows: Vec<(&str, &str)> = [
            ("earned", earned_removed.as_str()),
            ("paid", paid_removed.as_str()),
        ]
        .into_iter()
        .filter(|(_, p)| p.parse::<f64>().unwrap_or(0.0) > 0.0)
        .collect();
        if consume_rows.is_empty() {
            consume_rows.push(("paid", removed.as_str()));
        }
        for (bucket, portion) in consume_rows {
            sqlx::query(
                "INSERT INTO credit_ledger (address, kind, amount, bucket, captcha_ok) \
                 VALUES ($1, 'consume', $2::numeric, $3, FALSE)",
            )
            .bind(address)
            .bind(portion)
            .bind(bucket)
            .execute(&mut *tx)
            .await?;
        }

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
            if blocked {
                "user.block"
            } else {
                "user.unblock"
            },
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

    pub async fn admin_list_packs(&self) -> Result<Vec<PackAdminRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT sku, title, credits::text AS credits, price_cents, currency, \
                    active, sort_order \
             FROM credit_packs ORDER BY sort_order, price_cents, sku",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(map_pack_admin).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_create_pack(
        &self,
        sku: &str,
        title: &str,
        credits: &str,
        price_cents: i64,
        currency: &str,
        active: bool,
        sort_order: i32,
        detail: &JsonValue,
    ) -> Result<PackAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let exists = sqlx::query("SELECT 1 FROM credit_packs WHERE sku = $1")
            .bind(sku)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if exists {
            tx.rollback().await?;
            return Err(ApiError::conflict("pack sku already exists"));
        }
        let row = sqlx::query(
            "INSERT INTO credit_packs \
                 (sku, title, credits, price_cents, currency, active, sort_order) \
             VALUES ($1, $2, $3::numeric, $4, $5, $6, $7) \
             RETURNING sku, title, credits::text AS credits, price_cents, currency, \
                       active, sort_order",
        )
        .bind(sku)
        .bind(title)
        .bind(credits)
        .bind(price_cents)
        .bind(currency)
        .bind(active)
        .bind(sort_order)
        .fetch_one(&mut *tx)
        .await?;
        let pack = map_pack_admin(row);
        Self::audit(
            &mut *tx,
            "pack.create",
            None,
            None,
            Some(credits),
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(pack)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn admin_update_pack(
        &self,
        sku: &str,
        title: &str,
        credits: &str,
        price_cents: i64,
        currency: &str,
        active: bool,
        sort_order: i32,
        detail: &JsonValue,
    ) -> Result<PackAdminRow, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE credit_packs SET \
                 title = $2, credits = $3::numeric, price_cents = $4, \
                 currency = $5, active = $6, sort_order = $7 \
             WHERE sku = $1 \
             RETURNING sku, title, credits::text AS credits, price_cents, currency, \
                       active, sort_order",
        )
        .bind(sku)
        .bind(title)
        .bind(credits)
        .bind(price_cents)
        .bind(currency)
        .bind(active)
        .bind(sort_order)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or_else(|| ApiError::not_found("pack not found"))?;
        let pack = map_pack_admin(row);
        Self::audit(
            &mut *tx,
            "pack.update",
            None,
            None,
            Some(credits),
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(pack)
    }

    pub async fn admin_delete_pack(&self, sku: &str, detail: &JsonValue) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query("DELETE FROM credit_packs WHERE sku = $1")
            .bind(sku)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(ApiError::not_found("pack not found"));
        }
        Self::audit(
            &mut *tx,
            "pack.delete",
            None,
            None,
            None,
            None,
            None,
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn admin_list_purchases(
        &self,
        status: Option<&str>,
        address: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PurchaseAdminRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, address, sku, credits::text AS credits, amount_cents, currency, \
                    stripe_payment_intent, method, status, created_at \
             FROM credit_purchases \
             WHERE ($1::text IS NULL OR status = $1) \
               AND ($2::text IS NULL OR address = $2) \
             ORDER BY id DESC LIMIT $3 OFFSET $4",
        )
        .bind(status)
        .bind(address)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PurchaseAdminRow {
                id: r.get("id"),
                address: r.get("address"),
                sku: r.get("sku"),
                credits: r.get("credits"),
                amount_cents: r.get("amount_cents"),
                currency: r.get("currency"),
                stripe_payment_intent: r.get("stripe_payment_intent"),
                method: r.get("method"),
                status: r.get("status"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn admin_list_checkouts(
        &self,
        address: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CheckoutAdminRow>, ApiError> {
        let head = sqlx::query(
            "SELECT id, address, total_credits::text AS total_credits, status, created_at \
             FROM checkouts \
             WHERE ($1::text IS NULL OR address = $1) \
               AND ($2::text IS NULL OR status = $2) \
             ORDER BY id DESC LIMIT $3 OFFSET $4",
        )
        .bind(address)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let mut checkouts: Vec<CheckoutAdminRow> = head
            .into_iter()
            .map(|r| CheckoutAdminRow {
                id: r.get("id"),
                address: r.get("address"),
                total_credits: r.get("total_credits"),
                status: r.get("status"),
                created_at: r.get("created_at"),
                lines: Vec::new(),
            })
            .collect();

        if checkouts.is_empty() {
            return Ok(checkouts);
        }

        let ids: Vec<i64> = checkouts.iter().map(|c| c.id).collect();
        let lines = sqlx::query(
            "SELECT id, checkout_id, item_id, urn, token_id, \
                    unit_price_credits::text AS unit_price_credits, mode, status, \
                    attempts, last_error, external_ref \
             FROM fulfillment_outbox \
             WHERE checkout_id = ANY($1::bigint[]) \
             ORDER BY checkout_id, id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;

        use std::collections::HashMap;
        let mut idx: HashMap<i64, usize> = HashMap::new();
        for (i, c) in checkouts.iter().enumerate() {
            idx.insert(c.id, i);
        }
        for l in lines {
            let cid: i64 = l.get("checkout_id");
            if let Some(&i) = idx.get(&cid) {
                checkouts[i].lines.push(OutboxLineRow {
                    id: l.get("id"),
                    item_id: l.get("item_id"),
                    urn: l.get("urn"),
                    token_id: l.get("token_id"),
                    unit_price_credits: l.get("unit_price_credits"),
                    mode: l.get("mode"),
                    status: l.get("status"),
                    attempts: l.get("attempts"),
                    last_error: l.get("last_error"),
                    external_ref: l.get("external_ref"),
                });
            }
        }
        Ok(checkouts)
    }

    pub async fn admin_list_ledger(
        &self,
        address: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LedgerEntryRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT id, address, kind, amount::text AS amount, tx_ref, created_at \
             FROM credit_ledger WHERE address = $1 \
             ORDER BY id DESC LIMIT $2 OFFSET $3",
        )
        .bind(address)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| LedgerEntryRow {
                id: r.get("id"),
                address: r.get("address"),
                kind: r.get("kind"),
                amount: r.get("amount"),
                tx_ref: r.get("tx_ref"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn admin_audit_op(
        &self,
        action: &str,
        address: Option<&str>,
        entity_id: Option<i64>,
        amount: Option<&str>,
        actor: Option<&str>,
        detail: &JsonValue,
    ) -> Result<(), ApiError> {
        Self::audit(
            &self.pool, action, address, entity_id, amount, None, actor, detail,
        )
        .await
    }

    pub async fn admin_force_fulfill(&self, checkout_id: i64) -> Result<u64, ApiError> {
        let mut tx = self.pool.begin().await?;
        let exists = sqlx::query("SELECT status FROM checkouts WHERE id = $1 FOR UPDATE")
            .bind(checkout_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = exists else {
            tx.rollback().await?;
            return Err(ApiError::not_found("checkout not found"));
        };
        let status: String = row.get("status");
        if status != "fulfilling" {
            tx.rollback().await?;
            return Err(ApiError::conflict(
                "checkout is not in a re-armable 'fulfilling' state",
            ));
        }

        let res = sqlx::query(
            "UPDATE fulfillment_outbox \
             SET status = 'pending', attempts = 0, last_error = NULL, updated_at = now() \
             WHERE checkout_id = $1 AND status <> 'confirmed'",
        )
        .bind(checkout_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE checkouts SET status = 'fulfilling', updated_at = now() WHERE id = $1")
            .bind(checkout_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(res.rows_affected())
    }

    pub async fn find_confirmed_line_by_ref(
        &self,
        escrow_ref: &str,
    ) -> Result<Option<(String, String)>, ApiError> {
        let row = sqlx::query(
            "SELECT c.address AS address, o.unit_price_credits::text AS amount \
             FROM fulfillment_outbox o \
             JOIN checkouts c ON c.id = o.checkout_id \
             WHERE o.external_ref = $1 AND o.status = 'confirmed' \
             ORDER BY o.id DESC LIMIT 1",
        )
        .bind(escrow_ref)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| (r.get::<String, _>("address"), r.get::<String, _>("amount"))))
    }
}

pub async fn fetch_usage_grant(
    usage_grants_pool: &sqlx::PgPool,
    escrow_ref: &str,
) -> Result<Option<UsageGrantRow>, ApiError> {
    let row = sqlx::query(
        "SELECT grantee_address, urn, token_id, collection, status, unlock_at \
         FROM marketplace.usage_grants WHERE escrow_ref = $1 \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(escrow_ref)
    .fetch_optional(usage_grants_pool)
    .await?;
    Ok(row.map(|r| UsageGrantRow {
        grantee_address: r.get("grantee_address"),
        urn: r.get("urn"),
        token_id: r.get("token_id"),
        collection: r.get("collection"),
        status: r.get("status"),
        unlock_at: r.get("unlock_at"),
    }))
}

pub async fn mark_usage_grant_released(
    usage_grants_pool: &sqlx::PgPool,
    escrow_ref: &str,
) -> Result<u64, ApiError> {
    let res = sqlx::query(
        "UPDATE marketplace.usage_grants SET status = 'released' \
         WHERE escrow_ref = $1 AND status = 'active'",
    )
    .bind(escrow_ref)
    .execute(usage_grants_pool)
    .await?;
    Ok(res.rows_affected())
}

fn map_pack_admin(r: sqlx::postgres::PgRow) -> PackAdminRow {
    PackAdminRow {
        sku: r.get("sku"),
        title: r.get("title"),
        credits: r.get("credits"),
        price_cents: r.get("price_cents"),
        currency: r.get("currency"),
        active: r.get("active"),
        sort_order: r.get("sort_order"),
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
        kind: r.get("kind"),
    }
}
