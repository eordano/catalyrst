use serde_json::json;
use sqlx::Row;

use crate::http::ApiError;
use crate::money::CreditAmount;
use crate::ports::admin::GrantOutcome;
use crate::ports::credits::CreditsComponent;

/// Ledger rows are written by PostgreSQL, from the same NUMERIC values that
/// moved the balance, filtered by a NUMERIC `> 0` predicate.
///
/// Every earlier version of this decided in Rust with
/// `portion.parse::<f64>().unwrap_or(0.0) > 0.0` while the effect was applied
/// in NUMERIC. Those two disagree: PostgreSQL says `1e-400::numeric > 0` is
/// TRUE, `f64::from_str("1e-400")` is `0.0`. The balance moved, the row was
/// omitted, and reconcile diverged forever. Keeping the predicate inside the
/// INSERT makes that divergence unrepresentable.
pub(crate) const LEDGER_SPLIT_INSERT: &str = "INSERT INTO credit_ledger \
         (address, kind, amount, tx_ref, bucket, captcha_ok) \
     SELECT $1, $5, v.amount, $4, v.bucket, FALSE \
     FROM (VALUES ('earned'::text, $2::numeric), ('paid'::text, $3::numeric)) \
          AS v(bucket, amount) \
     WHERE v.amount > 0 \
     ORDER BY v.bucket";

impl CreditsComponent {
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
        let amount = CreditAmount::parse_non_negative(amount)?;
        if amount.is_zero() {
            let available: String = sqlx::query(
                "SELECT COALESCE((SELECT available::text FROM user_credits WHERE address = $1), \
                 '0') AS available",
            )
            .bind(address)
            .fetch_one(&mut **tx)
            .await?
            .get("available");
            return Ok(GrantOutcome {
                available,
                applied: amount.to_string(),
                replayed: false,
            });
        }
        let amount = amount.as_str();
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

        let current = sqlx::query(
            "SELECT available::text AS available, \
                    (available - COALESCE(( \
                         SELECT SUM(usd_cents) FROM credit_authorizations \
                         WHERE address = $1 AND status = 'authorized' AND expires_at > now() \
                     ), 0)::numeric / 100 >= $2::numeric) AS sufficient, \
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

        sqlx::query(LEDGER_SPLIT_INSERT)
            .bind(address)
            .bind(&earned_spent)
            .bind(&paid_spent)
            .bind(tx_ref)
            .bind("spend")
            .execute(&mut **tx)
            .await?;

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
        let amount = CreditAmount::parse_positive(amount)?;
        let amount = amount.as_str();
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
                    "SELECT available::text AS available, \
                            COALESCE(applied, amount)::text AS applied, \
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
                    applied: prior.get("applied"),
                    replayed: true,
                });
            }
        }

        sqlx::query("SELECT 1 FROM user_credits WHERE address = $1 FOR UPDATE")
            .bind(address)
            .fetch_optional(&mut **tx)
            .await?;

        let split = sqlx::query(
            "WITH w AS ( \
                 SELECT COALESCE(SUM(amount) FILTER (WHERE kind = 'spend' AND bucket = 'earned'), 0) AS spent_earned, \
                        COALESCE(SUM(amount) FILTER (WHERE kind = 'refund' AND bucket = 'earned'), 0) AS refunded_earned, \
                        COALESCE(SUM(amount) FILTER (WHERE kind = 'spend'), 0) AS spent_total, \
                        COALESCE(SUM(amount) FILTER (WHERE kind = 'refund'), 0) AS refunded_total \
                 FROM credit_ledger \
                 WHERE address = $3 AND tx_ref = $1 AND kind IN ('spend', 'refund') \
             ), a AS ( \
                 SELECT LEAST($2::numeric, GREATEST(0, w.spent_total - w.refunded_total)) AS applied, \
                        GREATEST(0, w.spent_earned - w.refunded_earned) AS earned_window \
                 FROM w \
             ) \
             SELECT a.applied::text AS applied, \
                    LEAST(a.applied, a.earned_window)::text AS earned_back \
             FROM a",
        )
        .bind(tx_ref)
        .bind(amount)
        .bind(address)
        .fetch_one(&mut **tx)
        .await?;
        let applied: String = split.get("applied");
        let earned_back: String = split.get("earned_back");

        let row = sqlx::query(
            "INSERT INTO user_credits (address, available, earned_available, updated_at) \
             VALUES ($1, $2::numeric, $3::numeric, now()) \
             ON CONFLICT (address) DO UPDATE \
                 SET available = user_credits.available + $2::numeric, \
                     earned_available = user_credits.earned_available + $3::numeric, \
                     updated_at = now() \
             RETURNING available::text AS available, \
                       ($2::numeric - $3::numeric)::text AS paid_back",
        )
        .bind(address)
        .bind(&applied)
        .bind(&earned_back)
        .fetch_one(&mut **tx)
        .await?;
        let available: String = row.get("available");
        let paid_back: String = row.get("paid_back");

        sqlx::query(LEDGER_SPLIT_INSERT)
            .bind(address)
            .bind(&earned_back)
            .bind(&paid_back)
            .bind(tx_ref)
            .bind("refund")
            .execute(&mut **tx)
            .await?;

        let detail = json!({ "source": "refund", "txRef": tx_ref, "requested": amount });
        Self::audit(
            &mut **tx,
            "credits.refund",
            Some(address),
            None,
            Some(applied.as_str()),
            Some("credits refund"),
            Some("system"),
            &detail,
        )
        .await?;

        if let Some(key) = idempotency_key {
            sqlx::query(
                "UPDATE credit_refund_idempotency \
                 SET available = $2::numeric, applied = $3::numeric \
                 WHERE idempotency_key = $1",
            )
            .bind(key)
            .bind(&available)
            .bind(&applied)
            .execute(&mut **tx)
            .await?;
        }

        Ok(GrantOutcome {
            available,
            applied,
            replayed: false,
        })
    }

    /// Remove credits from a wallet as the compensation for a REVERSED fiat
    /// payment (Stripe refund / dispute / chargeback).
    ///
    /// This is the opposite of [`Self::refund_in_tx`], and picking the wrong one
    /// is a live money defect: `refund` ADDS credits, so compensating a chargeback
    /// with it paid the buyer twice -- they got the fiat back from Stripe, KEPT
    /// the credits, and were credited that amount AGAIN.
    ///
    /// Semantics, deliberately matching `admin_revoke_credits`:
    /// * the balance floors at zero (`user_credits` CHECKs `earned_available >= 0
    ///   AND earned_available <= available`, so a negative balance is not
    ///   representable);
    /// * whatever could not be clawed back because the buyer already spent it is
    ///   reported as `shortfall` -- a real, unrecovered loss that the caller must
    ///   surface;
    /// * the debit is split **paid-first** -- the OPPOSITE of the earned-first
    ///   spend rule -- and lands in the ledger as `consume` rows (a DEBIT kind) so
    ///   reconcile's signed sum keeps matching the balance.
    ///
    /// Why paid-first: this reverses a PURCHASE, and a purchase grants *paid*
    /// credits, so a buyer who charges back a pack does not lose credits they
    /// earned by playing while paid credits sit untouched. The SQL expresses it as
    /// `earned_available = LEAST(earned_available, GREATEST(available - amount,
    /// 0))`. Pinned by `revoke_debits_the_paid_bucket_first` in
    /// `tests/formal_money.rs`; an earlier revision of this comment claimed
    /// earned-first and contradicted the statement below.
    ///
    /// TODO(owner-decision): flooring at zero is the SAFE choice, not necessarily
    /// the intended one. The alternative -- letting a chargeback drive the wallet
    /// negative so the debt follows the buyer -- cannot be expressed today and
    /// would need a schema change plus a policy on how a negative balance
    /// interacts with claiming and checkout. Until then a shortfall is an
    /// unrecovered loss, reported and logged at error level rather than carried.
    pub(crate) async fn revoke_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        address: &str,
        amount: &str,
        tx_ref: &str,
        reason: &str,
        detail: &serde_json::Value,
    ) -> Result<RevokeOutcome, ApiError> {
        let amount = CreditAmount::parse_positive(amount)?;
        let amount = amount.as_str();

        let before = sqlx::query(
            "SELECT available::text AS available, earned_available::text AS earned \
             FROM user_credits WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .fetch_optional(&mut **tx)
        .await?;

        let Some(before) = before else {
            let mut detail = detail.clone();
            if let serde_json::Value::Object(map) = &mut detail {
                map.insert("txRef".into(), json!(tx_ref));
                map.insert("chargedBack".into(), json!(amount));
                map.insert("removed".into(), json!("0"));
                map.insert("shortfall".into(), json!(amount));
                map.insert("walletRow".into(), json!("missing"));
            }
            Self::audit(
                &mut **tx,
                "credits.revoke",
                Some(address),
                None,
                Some("0"),
                Some(reason),
                Some("system"),
                &detail,
            )
            .await?;
            tracing::error!(
                address = %address,
                tx_ref = %tx_ref,
                charged_back = %amount,
                "credits revoke found NO wallet row: the entire charge-back is an \
                 unrecovered loss"
            );
            return Ok(RevokeOutcome {
                available: "0".to_string(),
                removed: "0".to_string(),
                shortfall: amount.to_string(),
                has_shortfall: true,
            });
        };
        let available_before: String = before.get("available");
        let earned_before: String = before.get("earned");

        let row = sqlx::query(
            "UPDATE user_credits \
             SET available = GREATEST(available - $2::numeric, 0), \
                 earned_available = LEAST(earned_available, GREATEST(available - $2::numeric, 0)), \
                 updated_at = now() \
             WHERE address = $1 \
             RETURNING available::text AS available, \
                       ($3::numeric - available)::text AS removed, \
                       ($4::numeric - earned_available)::text AS earned_removed, \
                       (($3::numeric - available) - ($4::numeric - earned_available))::text \
                           AS paid_removed, \
                       GREATEST($2::numeric - ($3::numeric - available), 0)::text AS shortfall, \
                       ($2::numeric > $3::numeric - available) AS has_shortfall",
        )
        .bind(address)
        .bind(amount)
        .bind(&available_before)
        .bind(&earned_before)
        .fetch_one(&mut **tx)
        .await?;
        let available: String = row.get("available");
        let removed: String = row.get("removed");
        let earned_removed: String = row.get("earned_removed");
        let paid_removed: String = row.get("paid_removed");
        let shortfall: String = row.get("shortfall");
        let has_shortfall: bool = row.get("has_shortfall");

        sqlx::query(LEDGER_SPLIT_INSERT)
            .bind(address)
            .bind(&earned_removed)
            .bind(&paid_removed)
            .bind(tx_ref)
            .bind("consume")
            .execute(&mut **tx)
            .await?;

        let mut detail = detail.clone();
        if let serde_json::Value::Object(map) = &mut detail {
            map.insert("txRef".into(), json!(tx_ref));
            map.insert("chargedBack".into(), json!(amount));
            map.insert("removed".into(), json!(removed));
            map.insert("shortfall".into(), json!(shortfall));
        }
        Self::audit(
            &mut **tx,
            "credits.revoke",
            Some(address),
            None,
            Some(removed.as_str()),
            Some(reason),
            Some("system"),
            &detail,
        )
        .await?;

        Ok(RevokeOutcome {
            available,
            removed,
            shortfall,
            has_shortfall,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RevokeOutcome {
    pub available: String,
    pub removed: String,
    /// Credits that could NOT be removed because the buyer had already spent
    /// them. Non-zero means an unrecovered loss; callers must log it loudly.
    pub shortfall: String,
    /// Whether `shortfall` is strictly positive, decided by PostgreSQL in
    /// NUMERIC (the text form may render as "0.00").
    pub has_shortfall: bool,
}
