use serde_json::json;
use sqlx::Row;

use crate::http::ApiError;
use crate::ports::admin::GrantOutcome;
use crate::ports::credits::CreditsComponent;

impl CreditsComponent {
    pub(crate) async fn expire_earned_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        address: &str,
    ) -> Result<String, ApiError> {
        let row = sqlx::query(
            "SELECT earned_available::text AS earned FROM user_credits \
             WHERE address = $1 AND earned_available > 0 \
               AND earned_expires_at IS NOT NULL AND earned_expires_at < now()",
        )
        .bind(address)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(row) = row else {
            return Ok("0".to_string());
        };
        let expired: String = row.get("earned");

        sqlx::query(
            "UPDATE user_credits \
             SET available = available - earned_available, earned_available = 0, \
                 earned_expires_at = NULL, updated_at = now() \
             WHERE address = $1",
        )
        .bind(address)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
             VALUES ($1, 'expire', $2::numeric, 'season-expiry', 'earned', FALSE)",
        )
        .bind(address)
        .bind(&expired)
        .execute(&mut **tx)
        .await?;
        tracing::info!(address, amount = %expired, "expired end-of-season earned credits");
        Ok(expired)
    }

    pub async fn balance(&self, address: &str) -> Result<String, ApiError> {
        let row =
            sqlx::query("SELECT available::text AS available FROM user_credits WHERE address = $1")
                .bind(address)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row
            .map(|r| r.get::<String, _>("available"))
            .unwrap_or_else(|| "0".to_string()))
    }

    pub async fn spend(
        &self,
        address: &str,
        amount: &str,
        tx_ref: &str,
        idempotency_key: Option<&str>,
    ) -> Result<GrantOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .spend_in_tx(&mut tx, address, amount, tx_ref, idempotency_key)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    pub(crate) async fn spend_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        address: &str,
        amount: &str,
        tx_ref: &str,
        idempotency_key: Option<&str>,
    ) -> Result<GrantOutcome, ApiError> {
        if let Some(key) = idempotency_key {
            let claimed = sqlx::query(
                "INSERT INTO credit_spend_idempotency \
                     (idempotency_key, address, amount, available, tx_ref) \
                 VALUES ($1, $2, $3::numeric, 0, $4) \
                 ON CONFLICT (idempotency_key) DO NOTHING \
                 RETURNING idempotency_key",
            )
            .bind(key)
            .bind(address)
            .bind(amount)
            .bind(tx_ref)
            .fetch_optional(&mut **tx)
            .await?
            .is_some();

            if !claimed {
                let prior = sqlx::query(
                    "SELECT available::text AS available, amount::text AS amount, \
                            (lower(address) = lower($2)) AS addr_match, \
                            (amount = $3::numeric) AS amount_match \
                     FROM credit_spend_idempotency WHERE idempotency_key = $1",
                )
                .bind(key)
                .bind(address)
                .bind(amount)
                .fetch_one(&mut **tx)
                .await?;
                let addr_match: bool = prior.get("addr_match");
                let amount_match: bool = prior.get("amount_match");
                if !addr_match || !amount_match {
                    return Err(ApiError::conflict(
                        "idempotency key already used for a different spend (address/amount mismatch)",
                    ));
                }
                return Ok(GrantOutcome {
                    available: prior.get("available"),
                    applied: prior.get("amount"),
                    replayed: true,
                });
            }
        }

        sqlx::query("SELECT 1 FROM user_credits WHERE address = $1 FOR UPDATE")
            .bind(address)
            .fetch_optional(&mut **tx)
            .await?;
        self.expire_earned_in_tx(tx, address).await?;

        let current = sqlx::query(
            "SELECT available::text AS available, (available >= $2::numeric) AS sufficient, \
                    LEAST(earned_available, $2::numeric)::text AS earned_spent, \
                    ($2::numeric - LEAST(earned_available, $2::numeric))::text AS paid_spent \
             FROM user_credits WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .bind(amount)
        .fetch_optional(&mut **tx)
        .await?;

        let Some(current) = current else {
            return Err(ApiError::payment_required("insufficient credits balance"));
        };
        let sufficient: bool = current.get("sufficient");
        if !sufficient {
            return Err(ApiError::payment_required("insufficient credits balance"));
        }
        let earned_spent: String = current.get("earned_spent");
        let paid_spent: String = current.get("paid_spent");

        let row = sqlx::query(
            "UPDATE user_credits \
             SET available = available - $2::numeric, \
                 earned_available = earned_available - LEAST(earned_available, $2::numeric), \
                 updated_at = now() \
             WHERE address = $1 \
             RETURNING available::text AS available",
        )
        .bind(address)
        .bind(amount)
        .fetch_one(&mut **tx)
        .await?;
        let available: String = row.get("available");

        let mut rows: Vec<(&str, &String)> = [("earned", &earned_spent), ("paid", &paid_spent)]
            .into_iter()
            .filter(|(_, p)| p.parse::<f64>().unwrap_or(0.0) > 0.0)
            .collect();
        if rows.is_empty() {
            rows.push(("paid", &paid_spent));
        }
        for (bucket, portion) in rows {
            sqlx::query(
                "INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
                 VALUES ($1, 'spend', $2::numeric, $3, $4, FALSE)",
            )
            .bind(address)
            .bind(portion)
            .bind(tx_ref)
            .bind(bucket)
            .execute(&mut **tx)
            .await?;
        }

        if let Some(key) = idempotency_key {
            sqlx::query(
                "UPDATE credit_spend_idempotency \
                 SET available = $2::numeric WHERE idempotency_key = $1",
            )
            .bind(key)
            .bind(&available)
            .execute(&mut **tx)
            .await?;
        }

        Ok(GrantOutcome {
            available,
            applied: amount.to_string(),
            replayed: false,
        })
    }

    pub async fn refund(
        &self,
        address: &str,
        amount: &str,
        tx_ref: &str,
        idempotency_key: Option<&str>,
    ) -> Result<GrantOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .refund_in_tx(&mut tx, address, amount, tx_ref, idempotency_key)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    pub(crate) async fn refund_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        address: &str,
        amount: &str,
        tx_ref: &str,
        idempotency_key: Option<&str>,
    ) -> Result<GrantOutcome, ApiError> {
        if let Some(key) = idempotency_key {
            let claimed = sqlx::query(
                "INSERT INTO credit_refund_idempotency \
                     (idempotency_key, address, amount, available, tx_ref) \
                 VALUES ($1, $2, $3::numeric, 0, $4) \
                 ON CONFLICT (idempotency_key) DO NOTHING \
                 RETURNING idempotency_key",
            )
            .bind(key)
            .bind(address)
            .bind(amount)
            .bind(tx_ref)
            .fetch_optional(&mut **tx)
            .await?
            .is_some();

            if !claimed {
                let prior = sqlx::query(
                    "SELECT available::text AS available, amount::text AS amount, \
                            (lower(address) = lower($2)) AS addr_match, \
                            (amount = $3::numeric) AS amount_match \
                     FROM credit_refund_idempotency WHERE idempotency_key = $1",
                )
                .bind(key)
                .bind(address)
                .bind(amount)
                .fetch_one(&mut **tx)
                .await?;
                let addr_match: bool = prior.get("addr_match");
                let amount_match: bool = prior.get("amount_match");
                if !addr_match || !amount_match {
                    return Err(ApiError::conflict(
                        "idempotency key already used for a different refund (address/amount mismatch)",
                    ));
                }
                return Ok(GrantOutcome {
                    available: prior.get("available"),
                    applied: prior.get("amount"),
                    replayed: true,
                });
            }
        }

        sqlx::query("SELECT 1 FROM user_credits WHERE address = $1 FOR UPDATE")
            .bind(address)
            .fetch_optional(&mut **tx)
            .await?;

        let split = sqlx::query(
            "SELECT GREATEST(0, LEAST( \
                        $2::numeric, \
                        COALESCE((SELECT SUM(amount) FROM credit_ledger \
                                  WHERE tx_ref = $1 AND kind = 'spend' AND bucket = 'earned'), 0) \
                        - COALESCE((SELECT SUM(amount) FROM credit_ledger \
                                    WHERE tx_ref = $1 AND kind = 'refund' AND bucket = 'earned'), 0) \
                    ))::text AS earned_back, \
                    (SELECT end_date FROM credits_seasons \
                     WHERE start_date <= now() AND end_date >= now() \
                     ORDER BY start_date DESC LIMIT 1) AS season_end",
        )
        .bind(tx_ref)
        .bind(amount)
        .fetch_one(&mut **tx)
        .await?;
        let season_end: Option<chrono::DateTime<chrono::Utc>> = split.get("season_end");
        let earned_back: String = match season_end {
            Some(_) => split.get("earned_back"),
            None => "0".to_string(),
        };

        let row = sqlx::query(
            "INSERT INTO user_credits (address, available, earned_available, earned_expires_at, updated_at) \
             VALUES ($1, $2::numeric, $3::numeric, CASE WHEN $3::numeric > 0 THEN $4 END, now()) \
             ON CONFLICT (address) DO UPDATE \
                 SET available = user_credits.available + $2::numeric, \
                     earned_available = user_credits.earned_available + $3::numeric, \
                     earned_expires_at = CASE \
                         WHEN user_credits.earned_available + $3::numeric > 0 \
                             THEN COALESCE($4, user_credits.earned_expires_at) \
                         ELSE user_credits.earned_expires_at END, \
                     updated_at = now() \
             RETURNING available::text AS available, \
                       ($2::numeric - $3::numeric)::text AS paid_back",
        )
        .bind(address)
        .bind(amount)
        .bind(&earned_back)
        .bind(season_end)
        .fetch_one(&mut **tx)
        .await?;
        let available: String = row.get("available");
        let paid_back: String = row.get("paid_back");

        let mut refund_rows: Vec<(&str, &String)> =
            [("earned", &earned_back), ("paid", &paid_back)]
                .into_iter()
                .filter(|(_, p)| p.parse::<f64>().unwrap_or(0.0) > 0.0)
                .collect();
        if refund_rows.is_empty() {
            refund_rows.push(("paid", &paid_back));
        }
        for (bucket, portion) in refund_rows {
            sqlx::query(
                "INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
                 VALUES ($1, 'refund', $2::numeric, $3, $4, FALSE)",
            )
            .bind(address)
            .bind(portion)
            .bind(tx_ref)
            .bind(bucket)
            .execute(&mut **tx)
            .await?;
        }

        let detail = json!({ "source": "refund", "txRef": tx_ref });
        Self::audit(
            &mut **tx,
            "credits.refund",
            Some(address),
            None,
            Some(amount),
            Some("credits refund"),
            Some("system"),
            &detail,
        )
        .await?;

        if let Some(key) = idempotency_key {
            sqlx::query(
                "UPDATE credit_refund_idempotency \
                 SET available = $2::numeric WHERE idempotency_key = $1",
            )
            .bind(key)
            .bind(&available)
            .execute(&mut **tx)
            .await?;
        }

        Ok(GrantOutcome {
            available,
            applied: amount.to_string(),
            replayed: false,
        })
    }
}
