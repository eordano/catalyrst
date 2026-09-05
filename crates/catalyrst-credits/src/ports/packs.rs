use serde_json::Value as JsonValue;
use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;

#[derive(Debug, Clone)]
pub enum MarkPaidOutcome {
    Granted {
        address: String,
        credits: String,
    },
    AmountMismatch {
        expected_cents: i64,
        charged_cents: i64,
    },
    NoPendingPurchase,
}

/// Result of compensating a REVERSED fiat payment (Stripe refund / dispute).
///
/// A reversal returns the buyer's money, so our side must REMOVE the credits
/// that purchase granted. This used to be modelled as a "refund" and executed
/// through the wallet refund path, which ADDS credits -- paying the buyer twice.
#[derive(Debug, Clone)]
pub enum ReversalOutcome {
    Reversed {
        address: String,
        /// Credits charged back against the purchase. Bounded by the purchase:
        /// cumulative charge-backs can never exceed the credits it granted.
        charged_back: String,
        /// Credits actually removed from the wallet (`charged_back` minus
        /// whatever the buyer had already spent).
        removed: String,
        /// Credits that could not be clawed back -- an unrecovered loss.
        shortfall: String,
        /// Whether `shortfall` is strictly positive (decided in NUMERIC).
        has_shortfall: bool,
    },
    NothingToReverse,
    NoPaidPurchase,
}

/// How much of the purchase this event reverses.
enum ReversalTarget {
    /// Stripe `charge.refunded` carries the CUMULATIVE refunded minor units.
    Cumulative(i64),
    /// A dispute/chargeback reverses the whole charge.
    Full,
}

#[derive(Debug, Clone)]
pub struct PackRow {
    pub sku: String,
    pub title: String,

    pub credits: String,

    pub price_cents: i64,
    pub currency: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone)]
pub struct OrderRow {
    pub order_id: String,
    pub address: String,
    pub sku: String,
    pub credits: String,
    pub amount_cents: i64,
    pub status: String,
    pub failure_reason: Option<String>,
}

impl CreditsComponent {
    pub async fn list_active_packs(&self) -> Result<Vec<PackRow>, ApiError> {
        let rows = sqlx::query(
            "SELECT sku, title, credits::text AS credits, price_cents, currency, sort_order \
             FROM credit_packs WHERE active = TRUE \
             ORDER BY sort_order, price_cents, sku",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(map_pack).collect())
    }

    pub async fn get_pack(&self, sku: &str) -> Result<Option<PackRow>, ApiError> {
        let row = sqlx::query(
            "SELECT sku, title, credits::text AS credits, price_cents, currency, sort_order \
             FROM credit_packs WHERE sku = $1 AND active = TRUE",
        )
        .bind(sku)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(map_pack))
    }

    pub async fn insert_pending_purchase(
        &self,
        address: &str,
        pack: &PackRow,
        payment_intent_id: &str,
    ) -> Result<i64, ApiError> {
        let row = sqlx::query(
            "INSERT INTO credit_purchases \
                 (address, sku, credits, amount_cents, currency, stripe_payment_intent, \
                  method, status) \
             VALUES ($1, $2, $3::numeric, $4, $5, $6, 'card', 'pending') \
             RETURNING id",
        )
        .bind(address)
        .bind(&pack.sku)
        .bind(&pack.credits)
        .bind(pack.price_cents)
        .bind(&pack.currency)
        .bind(payment_intent_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }

    pub async fn insert_pending_order(
        &self,
        order_id: &str,
        address: &str,
        pack: &PackRow,
        checkout_session_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO credit_purchases \
                 (order_id, address, sku, credits, amount_cents, currency, \
                  stripe_checkout_session, method, status) \
             VALUES ($1, $2, $3, $4::numeric, $5, $6, $7, 'card', 'pending')",
        )
        .bind(order_id)
        .bind(address)
        .bind(&pack.sku)
        .bind(&pack.credits)
        .bind(pack.price_cents)
        .bind(&pack.currency)
        .bind(checkout_session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_order(
        &self,
        order_id: &str,
        address: &str,
    ) -> Result<Option<OrderRow>, ApiError> {
        let row = sqlx::query(
            "SELECT order_id, address, sku, credits::text AS credits, amount_cents, \
                    status, failure_reason \
             FROM credit_purchases WHERE order_id = $1 AND address = $2",
        )
        .bind(order_id)
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| OrderRow {
            order_id: r.get("order_id"),
            address: r.get("address"),
            sku: r.get("sku"),
            credits: r.get("credits"),
            amount_cents: r.get("amount_cents"),
            status: r.get("status"),
            failure_reason: r.get("failure_reason"),
        }))
    }

    pub async fn mark_order_paid_by_session(
        &self,
        session_id: &str,
        event_id: &str,
        charged_cents: i64,
    ) -> Result<MarkPaidOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT address, credits::text AS credits, amount_cents, status, stripe_event_id \
             FROM credit_purchases WHERE stripe_checkout_session = $1 FOR UPDATE",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::NoPendingPurchase);
        };
        let address: String = row.get("address");
        let credits: String = row.get("credits");
        let amount_cents: i64 = row.get("amount_cents");
        let status: String = row.get("status");
        let existing_event: Option<String> = row.get("stripe_event_id");

        if status == "paid" && existing_event.as_deref() == Some(event_id) {
            tx.commit().await?;
            return Ok(MarkPaidOutcome::Granted { address, credits });
        }
        if status != "pending" {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::NoPendingPurchase);
        }
        if amount_cents != charged_cents {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::AmountMismatch {
                expected_cents: amount_cents,
                charged_cents,
            });
        }

        sqlx::query(
            "UPDATE credit_purchases \
             SET status = 'paid', stripe_event_id = $2, updated_at = now() \
             WHERE stripe_checkout_session = $1",
        )
        .bind(session_id)
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(MarkPaidOutcome::Granted { address, credits })
    }

    pub async fn mark_order_status_by_session(
        &self,
        session_id: &str,
        status: &str,
        failure_reason: Option<&str>,
    ) -> Result<bool, ApiError> {
        let res = sqlx::query(
            "UPDATE credit_purchases \
             SET status = $2, failure_reason = COALESCE($3, failure_reason), updated_at = now() \
             WHERE stripe_checkout_session = $1 AND status = 'pending'",
        )
        .bind(session_id)
        .bind(status)
        .bind(failure_reason)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn mark_purchase_paid(
        &self,
        payment_intent_id: &str,
        event_id: &str,
        charged_cents: i64,
    ) -> Result<MarkPaidOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT address, credits::text AS credits, amount_cents, status, stripe_event_id \
             FROM credit_purchases WHERE stripe_payment_intent = $1 FOR UPDATE",
        )
        .bind(payment_intent_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::NoPendingPurchase);
        };
        let address: String = row.get("address");
        let credits: String = row.get("credits");
        let amount_cents: i64 = row.get("amount_cents");
        let status: String = row.get("status");
        let existing_event: Option<String> = row.get("stripe_event_id");

        if status == "paid" && existing_event.as_deref() == Some(event_id) {
            tx.commit().await?;
            return Ok(MarkPaidOutcome::Granted { address, credits });
        }
        if status != "pending" {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::NoPendingPurchase);
        }
        if amount_cents != charged_cents {
            tx.rollback().await?;
            return Ok(MarkPaidOutcome::AmountMismatch {
                expected_cents: amount_cents,
                charged_cents,
            });
        }

        sqlx::query(
            "UPDATE credit_purchases \
             SET status = 'paid', stripe_event_id = $2, updated_at = now() \
             WHERE stripe_payment_intent = $1",
        )
        .bind(payment_intent_id)
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(MarkPaidOutcome::Granted { address, credits })
    }

    /// Stripe `charge.refunded`: `cumulative_refunded_cents` is the running
    /// total for the charge, so each delivery reverses only the increment.
    pub async fn record_charge_refund(
        &self,
        payment_intent_id: &str,
        cumulative_refunded_cents: i64,
        event_id: &str,
    ) -> Result<ReversalOutcome, ApiError> {
        self.apply_reversal(
            payment_intent_id,
            ReversalTarget::Cumulative(cumulative_refunded_cents),
            None,
            event_id,
        )
        .await
    }

    /// Stripe `charge.dispute.*`: the whole charge goes back to the buyer.
    pub async fn record_full_reversal(
        &self,
        payment_intent_id: &str,
        status_label: &str,
        event_id: &str,
    ) -> Result<ReversalOutcome, ApiError> {
        self.apply_reversal(
            payment_intent_id,
            ReversalTarget::Full,
            Some(status_label),
            event_id,
        )
        .await
    }

    /// Compensate a reversed fiat payment by REVOKING the credits it granted.
    ///
    /// Both defects this replaces were live:
    ///
    /// * it called `refund_in_tx`, which ADDS credits -- a chargeback returned
    ///   the buyer's money AND paid them the credits' worth a second time
    ///   while they kept the credits;
    /// * it passed the Stripe EVENT ID as the refund's `tx_ref`. Event ids
    ///   never carry spend rows, and the refund clamp only applied when spend
    ///   rows existed, so the amount was completely unbounded -- it could exceed
    ///   anything the wallet had ever spent or been granted.
    ///
    /// The replacement is bounded by the PURCHASE, not by spend rows:
    /// `credit_purchases.revoked_credits` accumulates the charge-backs and is
    /// held to `[0, credits]` by both `LEAST` here and a CHECK constraint
    /// (migration 0018), so cumulative charge-backs can never exceed what the
    /// purchase granted no matter how the webhook is replayed or reordered.
    ///
    /// TODO(owner-decision): this changes MONEY SEMANTICS and needs ratifying.
    /// (a) A Stripe reversal now REVOKES the granted credits instead of
    ///     crediting them. The safe reading of "the buyer's money went back";
    ///     confirm no downstream consumer depended on the old (double-paying)
    ///     behaviour, and that `credits.revoke` audit rows are what finance
    ///     expects to see for chargebacks.
    /// (b) The terminal event settles on the exact remaining `credits` rather
    ///     than another rounded proportion, so a fully reversed purchase lands
    ///     on `revoked_credits = credits` with no sub-cent dust left behind.
    ///     Migration 0010's comment assumed the proportional sum was already
    ///     exact; it is not.
    async fn apply_reversal(
        &self,
        payment_intent_id: &str,
        target: ReversalTarget,
        status_label: Option<&str>,
        event_id: &str,
    ) -> Result<ReversalOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT address, amount_cents, refunded_cents, \
                    revoked_credits::text AS revoked_credits, status \
             FROM credit_purchases WHERE stripe_payment_intent = $1 FOR UPDATE",
        )
        .bind(payment_intent_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(ReversalOutcome::NoPaidPurchase);
        };
        let address: String = row.get("address");
        let amount_cents: i64 = row.get("amount_cents");
        let prior_refunded: i64 = row.get("refunded_cents");
        let prior_revoked: String = row.get("revoked_credits");
        let status: String = row.get("status");

        if status != "paid" {
            tx.rollback().await?;
            return Ok(ReversalOutcome::NoPaidPurchase);
        }

        let new_refunded = match target {
            ReversalTarget::Cumulative(c) => c.clamp(prior_refunded, amount_cents),
            ReversalTarget::Full => amount_cents,
        };
        if new_refunded <= prior_refunded {
            // Redelivery of an event whose fiat increment is already recorded.
            tx.rollback().await?;
            return Ok(ReversalOutcome::NothingToReverse);
        }

        // Per-event charge-back: `credits * delta_cents / amount_cents` in
        // NUMERIC for a partial, and the EXACT remaining `credits` once the
        // cumulative fiat refund reaches the full charge. Settling the terminal
        // event on the remainder rather than on another rounded proportion
        // removes the sub-cent dust that migration 0010's "lossless" claim
        // overstated away (100 credits over 999 cents refunded as 3x333 sums to
        // 99.9999..., not 100), so a fully reversed purchase always ends at
        // `revoked_credits = credits` exactly.
        //
        // `has_charge` is decided by PostgreSQL in NUMERIC, never in f64.
        let upd = sqlx::query(
            "UPDATE credit_purchases \
             SET refunded_cents = $2, \
                 revoked_credits = LEAST(credits, \
                     CASE WHEN amount_cents <= 0 OR $2 >= amount_cents THEN credits \
                          ELSE revoked_credits \
                               + (credits * ($2 - $3)::numeric / amount_cents) END), \
                 status = CASE WHEN $4::text IS NOT NULL THEN $4 \
                               WHEN $2 >= amount_cents THEN 'refunded' \
                               ELSE status END, \
                 updated_at = now() \
             WHERE stripe_payment_intent = $1 \
             RETURNING (revoked_credits - $5::numeric)::text AS charged_back, \
                       (revoked_credits > $5::numeric) AS has_charge",
        )
        .bind(payment_intent_id)
        .bind(new_refunded)
        .bind(prior_refunded)
        .bind(status_label)
        .bind(&prior_revoked)
        .fetch_one(&mut *tx)
        .await?;
        let charged_back: String = upd.get("charged_back");
        let has_charge: bool = upd.get("has_charge");

        if !has_charge {
            // The purchase's credits were already fully charged back; the fiat
            // bookkeeping above is still committed.
            tx.commit().await?;
            return Ok(ReversalOutcome::NothingToReverse);
        }

        let detail = serde_json::json!({
            "source": "stripe",
            "eventId": event_id,
            "paymentIntent": payment_intent_id,
            "refundedCents": new_refunded,
            "amountCents": amount_cents,
        });
        let outcome = self
            .revoke_in_tx(
                &mut tx,
                &address,
                &charged_back,
                &format!("stripe:{}", event_id),
                "stripe payment reversed (refund/dispute)",
                &detail,
            )
            .await?;
        tx.commit().await?;
        Ok(ReversalOutcome::Reversed {
            address,
            charged_back,
            removed: outcome.removed,
            shortfall: outcome.shortfall,
            has_shortfall: outcome.has_shortfall,
        })
    }

    pub async fn record_stripe_event(
        &self,
        event_id: &str,
        event_type: &str,
        payload: &JsonValue,
    ) -> Result<bool, ApiError> {
        let row = sqlx::query(
            "INSERT INTO stripe_events (event_id, type, payload) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (event_id) DO UPDATE SET type = stripe_events.type \
             RETURNING processed_at",
        )
        .bind(event_id)
        .bind(event_type)
        .bind(payload)
        .fetch_one(&self.pool)
        .await?;
        let processed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("processed_at");
        Ok(processed_at.is_none())
    }

    pub async fn mark_stripe_event_processed(&self, event_id: &str) -> Result<(), ApiError> {
        sqlx::query("UPDATE stripe_events SET processed_at = now() WHERE event_id = $1")
            .bind(event_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn map_pack(r: sqlx::postgres::PgRow) -> PackRow {
    PackRow {
        sku: r.get("sku"),
        title: r.get("title"),
        credits: r.get("credits"),
        price_cents: r.get("price_cents"),
        currency: r.get("currency"),
        sort_order: r.get("sort_order"),
    }
}
