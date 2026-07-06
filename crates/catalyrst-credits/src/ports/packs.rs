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

#[derive(Debug, Clone)]
pub enum RefundOutcome {
    Refund { address: String, credits: String },
    NothingToRefund,
    NoPaidPurchase,
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

    pub async fn record_charge_refund(
        &self,
        payment_intent_id: &str,
        cumulative_refunded_cents: i64,
        event_id: &str,
    ) -> Result<RefundOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT address, amount_cents, refunded_cents, status \
             FROM credit_purchases WHERE stripe_payment_intent = $1 FOR UPDATE",
        )
        .bind(payment_intent_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(RefundOutcome::NoPaidPurchase);
        };
        let address: String = row.get("address");
        let amount_cents: i64 = row.get("amount_cents");
        let prior_refunded: i64 = row.get("refunded_cents");
        let status: String = row.get("status");

        if status != "paid" {
            tx.rollback().await?;
            return Ok(RefundOutcome::NoPaidPurchase);
        }

        let new_refunded = cumulative_refunded_cents.clamp(prior_refunded, amount_cents);
        let delta = new_refunded - prior_refunded;
        if delta <= 0 {
            tx.rollback().await?;
            return Ok(RefundOutcome::NothingToRefund);
        }

        let upd = sqlx::query(
            "UPDATE credit_purchases \
             SET refunded_cents = $2, \
                 status = CASE WHEN $2 >= amount_cents THEN 'refunded' ELSE status END, \
                 updated_at = now() \
             WHERE stripe_payment_intent = $1 \
             RETURNING (credits * ($2 - $3)::numeric / amount_cents)::text AS credits_to_refund",
        )
        .bind(payment_intent_id)
        .bind(new_refunded)
        .bind(prior_refunded)
        .fetch_one(&mut *tx)
        .await?;
        let credits: String = upd.get("credits_to_refund");
        self.refund_in_tx(&mut tx, &address, &credits, event_id, Some(event_id))
            .await?;
        tx.commit().await?;
        Ok(RefundOutcome::Refund { address, credits })
    }

    pub async fn record_full_reversal(
        &self,
        payment_intent_id: &str,
        status_label: &str,
        event_id: &str,
    ) -> Result<RefundOutcome, ApiError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT amount_cents, refunded_cents, status \
             FROM credit_purchases WHERE stripe_payment_intent = $1 FOR UPDATE",
        )
        .bind(payment_intent_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(RefundOutcome::NoPaidPurchase);
        };
        let amount_cents: i64 = row.get("amount_cents");
        let prior_refunded: i64 = row.get("refunded_cents");
        let status: String = row.get("status");
        if status != "paid" {
            tx.rollback().await?;
            return Ok(RefundOutcome::NoPaidPurchase);
        }

        let upd = sqlx::query(
            "UPDATE credit_purchases \
             SET refunded_cents = amount_cents, status = $2, updated_at = now() \
             WHERE stripe_payment_intent = $1 \
             RETURNING address, \
                       (credits * (amount_cents - $3)::numeric / amount_cents)::text AS credits_to_refund",
        )
        .bind(payment_intent_id)
        .bind(status_label)
        .bind(prior_refunded)
        .fetch_one(&mut *tx)
        .await?;
        let address: String = upd.get("address");
        let credits: String = upd.get("credits_to_refund");
        if prior_refunded >= amount_cents {
            tx.commit().await?;
            return Ok(RefundOutcome::NothingToRefund);
        }
        self.refund_in_tx(&mut tx, &address, &credits, event_id, Some(event_id))
            .await?;
        tx.commit().await?;
        Ok(RefundOutcome::Refund { address, credits })
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
