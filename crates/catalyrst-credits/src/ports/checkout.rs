use std::time::Duration;

use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;
use crate::ports::pricing::PricingClient;

#[derive(Debug, Clone)]
pub struct CartItemRow {
    pub item_id: String,

    pub collection: String,
    pub urn: String,
    pub category: String,
    pub qty: i32,

    pub unit_price_credits: String,
}

#[derive(Debug, Clone)]
pub struct CartView {
    pub items: Vec<CartItemRow>,

    pub total_credits: String,
}

#[derive(Debug, Clone)]
pub struct RepricedLine {
    pub item_id: String,

    pub collection: String,
    pub urn: String,
    pub category: String,
    pub qty: i32,
    pub unit_price_credits: String,

    pub token_id: Option<String>,

    pub trade_id: Option<String>,

    pub basis_wei: Option<String>,

    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct CheckoutOutcome {
    pub id: i64,
    pub status: String,

    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct CheckoutRow {
    pub id: i64,
    pub address: String,
    pub total_credits: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CheckoutIdemRow {
    pub id: i64,
    pub address: String,
    pub status: String,
}

impl CreditsComponent {
    pub async fn get_or_create_cart(&self, address: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            "INSERT INTO carts (address) VALUES ($1) \
             ON CONFLICT (address) DO UPDATE SET updated_at = now() \
             RETURNING id",
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }

    pub async fn add_item(
        &self,
        address: &str,
        item_id: &str,
        collection: &str,
        urn: &str,
        category: &str,
        qty: i32,
        unit_price_credits: &str,
    ) -> Result<(), ApiError> {
        let cart_id = self.get_or_create_cart(address).await?;
        sqlx::query(
            "INSERT INTO cart_items \
                 (cart_id, item_id, collection, urn, category, qty, unit_price_credits) \
             VALUES ($1, $2, $3, $4, $5, $6, $7::numeric) \
             ON CONFLICT (cart_id, collection, item_id) DO UPDATE \
                 SET qty = EXCLUDED.qty, \
                     urn = EXCLUDED.urn, \
                     category = EXCLUDED.category, \
                     unit_price_credits = EXCLUDED.unit_price_credits",
        )
        .bind(cart_id)
        .bind(item_id)
        .bind(collection)
        .bind(urn)
        .bind(category)
        .bind(qty)
        .bind(unit_price_credits)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_item(
        &self,
        address: &str,
        collection: &str,
        item_id: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM cart_items ci \
             USING carts c \
             WHERE ci.cart_id = c.id AND c.address = $1 \
               AND ci.item_id = $2 \
               AND COALESCE(ci.collection, lower(split_part(ci.urn, ':', 5))) = $3",
        )
        .bind(address)
        .bind(item_id)
        .bind(collection)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_cart(&self, address: &str) -> Result<CartView, ApiError> {
        let rows = sqlx::query(
            "SELECT ci.item_id, \
                    COALESCE(ci.collection, lower(split_part(ci.urn, ':', 5))) AS collection, \
                    ci.urn, ci.category, ci.qty, \
                    ci.unit_price_credits::text AS unit_price_credits \
             FROM cart_items ci \
             JOIN carts c ON c.id = ci.cart_id \
             WHERE c.address = $1 \
             ORDER BY ci.added_at, ci.id",
        )
        .bind(address)
        .fetch_all(&self.pool)
        .await?;

        let items: Vec<CartItemRow> = rows
            .into_iter()
            .map(|r| CartItemRow {
                item_id: r.get("item_id"),
                collection: r.get("collection"),
                urn: r.get("urn"),
                category: r.get("category"),
                qty: r.get("qty"),
                unit_price_credits: r.get("unit_price_credits"),
            })
            .collect();

        let total: String = sqlx::query(
            "SELECT COALESCE(SUM(ci.unit_price_credits * ci.qty), 0)::text AS total \
             FROM cart_items ci \
             JOIN carts c ON c.id = ci.cart_id \
             WHERE c.address = $1",
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?
        .get("total");

        Ok(CartView {
            items,
            total_credits: total,
        })
    }

    pub async fn clear_cart(&self, address: &str) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM cart_items ci USING carts c \
             WHERE ci.cart_id = c.id AND c.address = $1",
        )
        .bind(address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_checkout(&self, id: i64) -> Result<Option<CheckoutRow>, ApiError> {
        let row = sqlx::query(
            "SELECT id, address, total_credits::text AS total_credits, status \
             FROM checkouts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| CheckoutRow {
            id: r.get("id"),
            address: r.get("address"),
            total_credits: r.get("total_credits"),
            status: r.get("status"),
        }))
    }

    pub async fn find_checkout_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<CheckoutIdemRow>, ApiError> {
        let row =
            sqlx::query("SELECT id, address, status FROM checkouts WHERE idempotency_key = $1")
                .bind(idempotency_key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| CheckoutIdemRow {
            id: r.get("id"),
            address: r.get("address"),
            status: r.get("status"),
        }))
    }

    #[tracing::instrument(
        skip_all,
        fields(address = %address, checkout_id = tracing::field::Empty)
    )]
    pub async fn run_checkout(
        &self,
        address: &str,
        idempotency_key: &str,
        repriced: &[RepricedLine],
    ) -> Result<CheckoutOutcome, ApiError> {
        if repriced.is_empty() {
            return Err(ApiError::bad_request("cart is empty"));
        }

        let mut tx = self.pool.begin().await?;

        let claimed = sqlx::query(
            "INSERT INTO checkouts (idempotency_key, address, status) \
             VALUES ($1, $2, 'reserving') \
             ON CONFLICT (idempotency_key) DO NOTHING \
             RETURNING id",
        )
        .bind(idempotency_key)
        .bind(address)
        .fetch_optional(&mut *tx)
        .await?;

        let checkout_id: i64 = match claimed {
            Some(r) => r.get("id"),
            None => {
                let prior = sqlx::query(
                    "SELECT id, status, (lower(address) = lower($2)) AS addr_match \
                     FROM checkouts WHERE idempotency_key = $1",
                )
                .bind(idempotency_key)
                .bind(address)
                .fetch_one(&mut *tx)
                .await?;
                let addr_match: bool = prior.get("addr_match");
                if !addr_match {
                    return Err(ApiError::conflict(
                        "Idempotency-Key already used by a different wallet",
                    ));
                }
                tx.commit().await?;
                return Ok(CheckoutOutcome {
                    id: prior.get("id"),
                    status: prior.get("status"),
                    replayed: true,
                });
            }
        };
        tracing::Span::current().record("checkout_id", checkout_id);

        let cart_id: Option<i64> = sqlx::query("SELECT id FROM carts WHERE address = $1")
            .bind(address)
            .fetch_optional(&mut *tx)
            .await?
            .map(|r| r.get("id"));
        let Some(cart_id) = cart_id else {
            sqlx::query("UPDATE checkouts SET status = 'failed', updated_at = now() WHERE id = $1")
                .bind(checkout_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Err(ApiError::bad_request("cart is empty"));
        };

        let item_ids: Vec<String> = repriced.iter().map(|l| l.item_id.clone()).collect();
        let collections: Vec<String> = repriced.iter().map(|l| l.collection.clone()).collect();
        let urns: Vec<String> = repriced.iter().map(|l| l.urn.clone()).collect();
        let token_ids: Vec<Option<String>> = repriced.iter().map(|l| l.token_id.clone()).collect();
        let trade_ids: Vec<Option<String>> = repriced.iter().map(|l| l.trade_id.clone()).collect();
        let basis_weis: Vec<Option<String>> =
            repriced.iter().map(|l| l.basis_wei.clone()).collect();
        let prices: Vec<String> = repriced
            .iter()
            .map(|l| l.unit_price_credits.clone())
            .collect();
        let qtys: Vec<i32> = repriced.iter().map(|l| l.qty).collect();
        let modes: Vec<String> = repriced.iter().map(|l| l.mode.clone()).collect();

        let total: String = sqlx::query(
            "SELECT COALESCE(SUM(p::numeric * q), 0)::text AS total \
             FROM unnest($1::text[], $2::int[]) AS t(p, q)",
        )
        .bind(&prices)
        .bind(&qtys)
        .fetch_one(&mut *tx)
        .await?
        .get("total");

        sqlx::query(
            "UPDATE checkouts SET total_credits = $2::numeric, updated_at = now() WHERE id = $1",
        )
        .bind(checkout_id)
        .bind(&total)
        .execute(&mut *tx)
        .await?;

        let bal = sqlx::query(
            "SELECT (available >= $2::numeric) AS sufficient \
             FROM user_credits WHERE address = $1 FOR UPDATE",
        )
        .bind(address)
        .bind(&total)
        .fetch_optional(&mut *tx)
        .await?;
        let sufficient = bal.map(|r| r.get::<bool, _>("sufficient")).unwrap_or(false);
        if !sufficient {
            sqlx::query("UPDATE checkouts SET status = 'failed', updated_at = now() WHERE id = $1")
                .bind(checkout_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Err(ApiError::payment_required("insufficient credits balance"));
        }

        let checkout_ref = format!("checkout:{}", checkout_id);
        self.spend_in_tx(
            &mut tx,
            address,
            &total,
            &checkout_ref,
            Some(idempotency_key),
        )
        .await?;

        let ledger_id: Option<i64> = sqlx::query(
            "SELECT id FROM credit_ledger \
             WHERE tx_ref = $1 AND kind = 'spend' ORDER BY id DESC LIMIT 1",
        )
        .bind(&checkout_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|r| r.get("id"));

        sqlx::query(
            "INSERT INTO fulfillment_outbox \
                 (checkout_id, item_id, collection, urn, token_id, trade_id, basis_wei, unit_price_credits, mode, status) \
             SELECT $1, t.item_id, t.collection, t.urn, t.token_id, t.trade_id, t.basis_wei, t.price::numeric, t.mode, 'pending' \
             FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[], $8::text[], $9::int[], $10::text[]) \
                      AS t(item_id, collection, urn, token_id, trade_id, basis_wei, price, qty, mode) \
             CROSS JOIN LATERAL generate_series(1, t.qty) AS g",
        )
        .bind(checkout_id)
        .bind(&item_ids)
        .bind(&collections)
        .bind(&urns)
        .bind(&token_ids)
        .bind(&trade_ids)
        .bind(&basis_weis)
        .bind(&prices)
        .bind(&qtys)
        .bind(&modes)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE checkouts SET status = 'fulfilling', ledger_id = $2, updated_at = now() \
             WHERE id = $1",
        )
        .bind(checkout_id)
        .bind(ledger_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "DELETE FROM cart_items ci \
             USING unnest($2::text[], $3::text[]) AS t(collection, item_id) \
             WHERE ci.cart_id = $1 \
               AND ci.item_id = t.item_id \
               AND COALESCE(ci.collection, lower(split_part(ci.urn, ':', 5))) = t.collection",
        )
        .bind(cart_id)
        .bind(&collections)
        .bind(&item_ids)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(CheckoutOutcome {
            id: checkout_id,
            status: "fulfilling".to_string(),
            replayed: false,
        })
    }
}

struct OutboxRow {
    id: i64,
    checkout_id: i64,
    item_id: String,

    collection: String,
    urn: String,
    token_id: Option<String>,

    trade_id: Option<String>,

    basis_wei: Option<String>,

    unit_price_credits: String,
    mode: String,
    attempts: i32,
    buyer: String,
}

enum BrokerResult {
    Confirmed {
        tx_hash: String,
        category: String,
        collection: String,
    },

    Retryable(String),

    Terminal(String),
}

#[derive(Clone)]
pub struct OutboxWorker {
    pub credits: CreditsComponent,
    pub pricing: PricingClient,
    pub http: reqwest::Client,
    pub economy_base_url: String,
    pub economy_admin_token: Option<String>,
    pub escrow_address: Option<String>,
    pub max_attempts: i32,

    pub usage_grants_pool: Option<sqlx::PgPool>,

    pub escrow_lock_days: i32,

    pub mock_fulfillment: bool,
}

impl OutboxWorker {
    pub fn spawn(self, interval_secs: u64) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                if let Err(e) = self.run_once().await {
                    tracing::warn!(error = %e, "checkout outbox drain failed");
                }
            }
        });
    }

    pub async fn run_once(&self) -> Result<usize, ApiError> {
        self.compensation_sweep().await?;

        let candidates = sqlx::query(
            "SELECT o.id \
             FROM fulfillment_outbox o \
             JOIN checkouts c ON c.id = o.checkout_id \
             WHERE o.status = 'pending' AND o.attempts < $1 AND c.status = 'fulfilling' \
             ORDER BY o.id LIMIT 50",
        )
        .bind(self.max_attempts)
        .fetch_all(&self.credits.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("id"))
        .collect::<Vec<i64>>();

        if candidates.is_empty() {
            return Ok(0);
        }

        let (Some(token), Some(escrow)) = (
            self.economy_admin_token.as_ref(),
            self.escrow_address.as_ref(),
        ) else {
            tracing::warn!(
                pending = candidates.len(),
                "fulfilment outbox has pending rows but ECONOMY token/escrow are unset; idling"
            );
            return Ok(0);
        };

        let mut processed = 0usize;
        for id in candidates {
            if self.process_one(id, token, escrow).await? {
                processed += 1;
            }
        }
        Ok(processed)
    }

    #[tracing::instrument(
        skip(self, token, escrow),
        fields(outbox_id = id, checkout_id = tracing::field::Empty)
    )]
    async fn process_one(&self, id: i64, token: &str, escrow: &str) -> Result<bool, ApiError> {
        let mut tx = self.credits.pool.begin().await?;

        let claimed = sqlx::query(
            "SELECT o.id, o.checkout_id, o.item_id, \
                    COALESCE(o.collection, lower(split_part(o.urn, ':', 5))) AS collection, \
                    o.urn, o.token_id, o.trade_id, o.basis_wei, \
                    o.unit_price_credits::text AS unit_price_credits, \
                    o.mode, o.attempts, \
                    c.address AS buyer \
             FROM fulfillment_outbox o \
             JOIN checkouts c ON c.id = o.checkout_id \
             WHERE o.id = $1 AND o.status = 'pending' AND c.status = 'fulfilling' \
             FOR UPDATE OF o SKIP LOCKED",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(r) = claimed else {
            tx.rollback().await?;
            return Ok(false);
        };
        let row = OutboxRow {
            id: r.get("id"),
            checkout_id: r.get("checkout_id"),
            item_id: r.get("item_id"),
            collection: r.get("collection"),
            urn: r.get("urn"),
            token_id: r.get("token_id"),
            trade_id: r.get("trade_id"),
            basis_wei: r.get("basis_wei"),
            unit_price_credits: r.get("unit_price_credits"),
            mode: r.get("mode"),
            attempts: r.get("attempts"),
            buyer: r.get("buyer"),
        };
        tracing::Span::current().record("checkout_id", row.checkout_id);

        let result = self.attempt_broker_buy(&row, token, escrow).await;

        match result {
            BrokerResult::Confirmed {
                tx_hash,
                category,
                collection,
            } => {
                sqlx::query(
                    "UPDATE fulfillment_outbox \
                     SET status = 'confirmed', external_ref = $2, updated_at = now() \
                     WHERE id = $1",
                )
                .bind(row.id)
                .bind(&tx_hash)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    "UPDATE checkouts SET status = 'fulfilled', updated_at = now() \
                     WHERE id = $1 AND status = 'fulfilling' \
                       AND NOT EXISTS ( \
                           SELECT 1 FROM fulfillment_outbox \
                           WHERE checkout_id = $1 AND status <> 'confirmed')",
                )
                .bind(row.checkout_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;

                tracing::info!(
                    checkout_id = row.checkout_id,
                    urn = %row.urn,
                    tx_hash = %tx_hash,
                    "fulfilment line confirmed on-chain"
                );

                self.write_usage_grant(
                    &row.buyer,
                    &row.urn,
                    row.token_id.as_deref(),
                    &category,
                    &collection,
                    &tx_hash,
                )
                .await;
            }
            BrokerResult::Retryable(err) => {
                let new_attempts = row.attempts + 1;
                if new_attempts >= self.max_attempts {
                    self.fail_and_freeze(&mut tx, row.id, row.checkout_id, &err)
                        .await?;
                    tx.commit().await?;
                    self.try_compensate(row.checkout_id, &row.buyer).await;
                } else {
                    sqlx::query(
                        "UPDATE fulfillment_outbox \
                         SET attempts = $2, last_error = $3, updated_at = now() \
                         WHERE id = $1",
                    )
                    .bind(row.id)
                    .bind(new_attempts)
                    .bind(truncate(&err, 1000))
                    .execute(&mut *tx)
                    .await?;
                    tx.commit().await?;
                }
            }
            BrokerResult::Terminal(err) => {
                self.fail_and_freeze(&mut tx, row.id, row.checkout_id, &err)
                    .await?;
                tx.commit().await?;
                self.try_compensate(row.checkout_id, &row.buyer).await;
            }
        }

        Ok(true)
    }

    async fn fail_and_freeze(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        outbox_id: i64,
        checkout_id: i64,
        err: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE fulfillment_outbox \
             SET status = 'failed', attempts = attempts + 1, last_error = $2, updated_at = now() \
             WHERE id = $1",
        )
        .bind(outbox_id)
        .bind(truncate(err, 1000))
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE checkouts SET status = 'reversing', updated_at = now() \
             WHERE id = $1 AND status = 'fulfilling'",
        )
        .bind(checkout_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn compensation_sweep(&self) -> Result<(), ApiError> {
        let rows = sqlx::query(
            "SELECT id, address FROM checkouts WHERE status = 'reversing' ORDER BY id LIMIT 50",
        )
        .fetch_all(&self.credits.pool)
        .await?;
        for r in rows {
            let checkout_id: i64 = r.get("id");
            let buyer: String = r.get("address");
            self.try_compensate(checkout_id, &buyer).await;
        }
        Ok(())
    }

    async fn try_compensate(&self, checkout_id: i64, buyer: &str) {
        if let Err(e) = self.compensate(checkout_id, buyer).await {
            tracing::warn!(
                checkout_id,
                error = %e,
                "compensation failed; checkout stays 'reversing' and will retry next tick"
            );
        }
    }

    #[tracing::instrument(skip(self, buyer))]
    async fn compensate(&self, checkout_id: i64, buyer: &str) -> Result<(), ApiError> {
        let agg = sqlx::query(
            "SELECT COALESCE(SUM(unit_price_credits) FILTER (WHERE status <> 'confirmed'), 0)::text \
                        AS refund_amt, \
                    COALESCE(SUM(unit_price_credits) FILTER (WHERE status <> 'confirmed'), 0) > 0 \
                        AS has_refund, \
                    bool_or(status = 'confirmed') AS any_confirmed \
             FROM fulfillment_outbox WHERE checkout_id = $1",
        )
        .bind(checkout_id)
        .fetch_one(&self.credits.pool)
        .await?;
        let refund_amt: String = agg.get("refund_amt");
        let has_refund: bool = agg.get("has_refund");
        let any_confirmed: bool = agg.get::<Option<bool>, _>("any_confirmed").unwrap_or(false);

        if has_refund {
            let idem = format!("reversal:{}", checkout_id);
            let tx_ref = format!("checkout:{}", checkout_id);
            self.credits
                .refund(buyer, &refund_amt, &tx_ref, Some(&idem))
                .await?;
        }

        let new_status = if any_confirmed { "failed" } else { "reversed" };
        sqlx::query(
            "UPDATE checkouts SET status = $2, updated_at = now() \
             WHERE id = $1 AND status = 'reversing'",
        )
        .bind(checkout_id)
        .bind(new_status)
        .execute(&self.credits.pool)
        .await?;

        if any_confirmed {
            tracing::error!(
                checkout_id,
                refund_credits = %refund_amt,
                "PARTIAL fulfilment reversed: a sibling line was already minted to escrow; the \
                 undelivered Credits were refunded and the confirmed line(s) need Phase-6 escrow \
                 reconciliation"
            );
        } else {
            tracing::warn!(
                checkout_id,
                refund_credits = %refund_amt,
                "checkout fulfilment failed terminally; Credits refunded and checkout reversed"
            );
        }
        Ok(())
    }

    async fn attempt_broker_buy(&self, row: &OutboxRow, token: &str, escrow: &str) -> BrokerResult {
        let info = match self.pricing.fetch_item(&row.collection, &row.item_id).await {
            Ok(i) => i,
            Err(ApiError::NotFound(m)) | Err(ApiError::BadRequest(m)) => {
                return BrokerResult::Terminal(format!("catalog rejected item: {m}"));
            }
            Err(e) => return BrokerResult::Retryable(format!("catalog fetch failed: {e}")),
        };

        let (price_wei, token_id, trade_payload): (
            String,
            Option<String>,
            Option<serde_json::Value>,
        ) = if row.mode == "trade" {
            let Some(trade_id) = row.trade_id.as_deref().filter(|t| !t.trim().is_empty()) else {
                return BrokerResult::Terminal(
                    "trade line without a pinned trade id — cannot fulfil; compensation will \
                     refund the line"
                        .to_string(),
                );
            };
            let Some(basis) = row.basis_wei.as_deref().map(str::trim).filter(|b| {
                !b.is_empty()
                    && b.bytes().all(|x| x.is_ascii_digit())
                    && crate::ports::pricing::payment_is_positive(b)
            }) else {
                return BrokerResult::Terminal(
                    "trade line without a pinned positive basis_wei — refusing an unpriced buy"
                        .to_string(),
                );
            };
            let trade = match self.pricing.fetch_trade(trade_id).await {
                Ok(t) => t,
                Err(ApiError::NotFound(m)) => {
                    return BrokerResult::Terminal(format!(
                        "trade {trade_id} vanished from the market book: {m}; compensation \
                         will refund the line"
                    ))
                }
                Err(e) => return BrokerResult::Retryable(format!("trade fetch failed: {e}")),
            };
            match trade.get("status").and_then(|s| s.as_str()) {
                Some("open") => {}
                other => {
                    return BrokerResult::Terminal(format!(
                        "trade {trade_id} is no longer open in the market book (status {other:?}); \
                         refusing a doomed on-chain accept — compensation will refund the line"
                    ))
                }
            }
            (basis.to_string(), None, Some(trade))
        } else if row.mode == "secondary" {
            match secondary_source(row.token_id.as_deref(), row.basis_wei.as_deref()) {
                SecondarySource::Pinned {
                    token_id,
                    price_wei,
                } => (price_wei, Some(token_id), None),
                SecondarySource::Fresh => {
                    let order = match self
                        .pricing
                        .fetch_open_order(&info.contract_address, &row.item_id)
                        .await
                    {
                        Ok(Some(order)) => order,
                        Ok(None) => {
                            return BrokerResult::Terminal(
                                "no fillable MarketplaceV2 listing for this item (secondary)"
                                    .to_string(),
                            )
                        }
                        Err(e) => {
                            return BrokerResult::Retryable(format!("order lookup failed: {e}"))
                        }
                    };
                    let mana_usd = match self.pricing.fetch_mana_usd().await {
                        Ok(v) => v,
                        Err(e) => {
                            return BrokerResult::Retryable(format!(
                                "oracle fetch for legacy-row reprice failed: {e}"
                            ))
                        }
                    };
                    let fresh_credits = match self
                        .pricing
                        .compute_credit_price(&self.credits.pool, &order.price_wei, &mana_usd)
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            return BrokerResult::Retryable(format!(
                                "legacy-row reprice failed: {e}"
                            ))
                        }
                    };
                    if fresh_price_exceeds_charge(&row.unit_price_credits, &fresh_credits) {
                        return BrokerResult::Terminal(format!(
                            "legacy unpinned line: current cheapest listing reprices to {} \
                             Credits, above the {} Credits the buyer was charged; refusing to \
                             overpay — compensation will refund the line",
                            fresh_credits, row.unit_price_credits
                        ));
                    }
                    (order.price_wei, Some(order.token_id), None)
                }
            }
        } else {
            if !info.store_mintable {
                return BrokerResult::Terminal(
                    "this item's mint is no longer available from its collection store (off \
                     sale, sold out, or moved to a trade); refusing a doomed on-chain buy — \
                     compensation will refund the line"
                        .to_string(),
                );
            }
            let price = match primary_source(row.basis_wei.as_deref()) {
                PrimarySource::Pinned(basis) => basis,
                PrimarySource::Fresh => info.price_wei.clone(),
            };
            (price, None, None)
        };

        if crate::ports::pricing::payment_is_positive(&price_wei)
            && !crate::ports::pricing::charge_is_positive(&row.unit_price_credits)
        {
            return BrokerResult::Terminal(format!(
                "charge/payment mismatch: line was charged {} Credits but fulfillment would pay \
                 {} wei; refusing to buy at the buyer's expense",
                row.unit_price_credits, price_wei
            ));
        }

        if self.mock_fulfillment {
            tracing::info!(
                checkout_id = %row.checkout_id,
                outbox_id = %row.id,
                "CREDITS_MOCK_FULFILLMENT on — delivering off-chain, no broker call"
            );
            return BrokerResult::Confirmed {
                tx_hash: format!("mock:checkout:{}:{}", row.checkout_id, row.id),
                category: info.category.clone(),
                collection: info.contract_address.clone(),
            };
        }

        let mut body = serde_json::json!({
            "collection": info.contract_address,
            "itemId": row.item_id,
            "priceWei": price_wei,
            "escrowAddress": escrow,
            "mode": row.mode,
            "buyerAddress": row.buyer,
        });
        if let Some(tid) = &token_id {
            body["tokenId"] = serde_json::Value::String(tid.clone());
        }
        if let Some(trade) = trade_payload {
            body["trade"] = trade;
        }

        let idem_key = format!("checkout:{}:{}", row.checkout_id, row.id);
        let url = format!("{}/v1/broker/buy", self.economy_base_url);
        let resp = match self
            .http
            .post(&url)
            .bearer_auth(token)
            .header("Idempotency-Key", idem_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return BrokerResult::Retryable(format!("broker request failed: {e}")),
        };

        let status = resp.status();
        if status.is_success() {
            let parsed: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => return BrokerResult::Retryable(format!("broker parse failed: {e}")),
            };
            match parsed.get("txHash").and_then(|v| v.as_str()) {
                Some(tx_hash) => BrokerResult::Confirmed {
                    tx_hash: tx_hash.to_string(),
                    category: info.category.clone(),
                    collection: info.contract_address.clone(),
                },
                None => BrokerResult::Retryable("broker 200 missing txHash".to_string()),
            }
        } else {
            let code = status.as_u16();
            let body_txt = resp.text().await.unwrap_or_default();
            let msg = format!("broker status {code}: {}", truncate(&body_txt, 300));

            if code == 408 || code == 409 || code == 429 || (500..600).contains(&code) {
                BrokerResult::Retryable(msg)
            } else {
                BrokerResult::Terminal(msg)
            }
        }
    }

    async fn write_usage_grant(
        &self,
        buyer: &str,
        urn: &str,
        token_id: Option<&str>,
        category: &str,
        collection: &str,
        escrow_ref: &str,
    ) {
        let Some(pool) = self.usage_grants_pool.as_ref() else {
            tracing::warn!(
                urn = %urn,
                "USAGE_GRANTS_PG_CONNECTION_STRING unset; skipping usage_grant write \
                 (the escrowed item will not render in the backpack until configured)"
            );
            return;
        };

        let res = sqlx::query(
            "INSERT INTO marketplace.usage_grants \
                 (grantee_address, urn, token_id, category, collection, escrow_ref, unlock_at, status) \
             VALUES (lower($1), $2, $3, $4, $5, $6, now() + make_interval(days => $7), 'active') \
             ON CONFLICT DO NOTHING",
        )
        .bind(buyer)
        .bind(urn)
        .bind(token_id)
        .bind(category)
        .bind(collection)
        .bind(escrow_ref)
        .bind(self.escrow_lock_days)
        .execute(pool)
        .await;

        match res {
            Ok(_) => tracing::info!(
                buyer = %buyer.to_lowercase(),
                urn = %urn,
                escrow_ref = %escrow_ref,
                "usage_grant written (escrow lease active)"
            ),
            Err(e) => tracing::error!(
                error = %e,
                urn = %urn,
                escrow_ref = %escrow_ref,
                "usage_grant write FAILED (line is confirmed; Phase-8 reconciliation backstops)"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SecondarySource {
    Pinned { token_id: String, price_wei: String },

    Fresh,
}

fn fresh_price_exceeds_charge(charged_credits: &str, fresh_credits: &str) -> bool {
    match (
        parse_nonneg_decimal(charged_credits),
        parse_nonneg_decimal(fresh_credits),
    ) {
        (Some(charged), Some(fresh)) => {
            cmp_decimal(&fresh, &charged) == std::cmp::Ordering::Greater
        }
        _ => true,
    }
}

fn parse_nonneg_decimal(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((
        int_part.trim_start_matches('0').to_string(),
        frac_part.trim_end_matches('0').to_string(),
    ))
}

fn cmp_decimal(a: &(String, String), b: &(String, String)) -> std::cmp::Ordering {
    a.0.len()
        .cmp(&b.0.len())
        .then_with(|| a.0.cmp(&b.0))
        .then_with(|| a.1.cmp(&b.1))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrimarySource {
    Pinned(String),

    Fresh,
}

fn primary_source(basis_wei: Option<&str>) -> PrimarySource {
    match basis_wei.map(str::trim) {
        Some(b) if !b.is_empty() && b.bytes().all(|x| x.is_ascii_digit()) => {
            PrimarySource::Pinned(b.to_string())
        }
        _ => PrimarySource::Fresh,
    }
}

fn secondary_source(token_id: Option<&str>, basis_wei: Option<&str>) -> SecondarySource {
    match (token_id, basis_wei) {
        (Some(t), Some(b))
            if !t.trim().is_empty() && crate::ports::pricing::payment_is_positive(b) =>
        {
            SecondarySource::Pinned {
                token_id: t.to_string(),
                price_wei: b.to_string(),
            }
        }
        _ => SecondarySource::Fresh,
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::pricing::{charge_is_positive, payment_is_positive};

    #[test]
    fn pinned_listing_wins_when_checkout_recorded_it() {
        assert_eq!(
            secondary_source(Some("2901"), Some("10000000000000000")),
            SecondarySource::Pinned {
                token_id: "2901".into(),
                price_wei: "10000000000000000".into(),
            }
        );
    }

    #[test]
    fn pinned_mint_basis_wins_for_primary_rows() {
        assert_eq!(
            primary_source(Some("2500000000000000000")),
            PrimarySource::Pinned("2500000000000000000".into())
        );
        assert_eq!(primary_source(Some("0")), PrimarySource::Pinned("0".into()));
        assert_eq!(
            primary_source(Some(" 123 ")),
            PrimarySource::Pinned("123".into())
        );
    }

    #[test]
    fn unpinned_primary_rows_fall_back_to_the_catalog_price() {
        assert_eq!(primary_source(None), PrimarySource::Fresh);
        assert_eq!(primary_source(Some("")), PrimarySource::Fresh);
        assert_eq!(primary_source(Some("  ")), PrimarySource::Fresh);
        assert_eq!(primary_source(Some("1.5")), PrimarySource::Fresh);
        assert_eq!(primary_source(Some("-1")), PrimarySource::Fresh);
        assert_eq!(primary_source(Some("1e18")), PrimarySource::Fresh);
    }

    #[test]
    fn legacy_rows_fall_back_to_fresh_selection() {
        assert_eq!(secondary_source(None, None), SecondarySource::Fresh);
        assert_eq!(secondary_source(Some("2901"), None), SecondarySource::Fresh);
        assert_eq!(
            secondary_source(None, Some("10000000000000000")),
            SecondarySource::Fresh
        );
        assert_eq!(
            secondary_source(Some("2901"), Some("0")),
            SecondarySource::Fresh
        );
        assert_eq!(
            secondary_source(Some(" "), Some("10000000000000000")),
            SecondarySource::Fresh
        );
    }

    #[test]
    fn fresh_listing_within_charge_proceeds() {
        assert!(!fresh_price_exceeds_charge("3", "3"), "equal → proceed");
        assert!(!fresh_price_exceeds_charge("3", "2"), "cheaper → proceed");
        assert!(!fresh_price_exceeds_charge("3", "2.99"));
        assert!(!fresh_price_exceeds_charge("1.5", "1.5"));
        assert!(!fresh_price_exceeds_charge("10", "9.999"));
    }

    #[test]
    fn fresh_listing_above_charge_is_refused_terminally() {
        assert!(fresh_price_exceeds_charge("3", "4"));
        assert!(fresh_price_exceeds_charge("3", "3.01"));
        assert!(fresh_price_exceeds_charge("1.5", "2"));
        assert!(fresh_price_exceeds_charge("9.999", "10"));
        assert!(fresh_price_exceeds_charge("abc", "1"));
        assert!(fresh_price_exceeds_charge("3", "-1"));
        assert!(fresh_price_exceeds_charge("", ""));
        assert!(fresh_price_exceeds_charge("3", "1e2"));
    }

    #[test]
    fn decimal_comparator_handles_zero_padding_and_lengths() {
        assert!(
            !fresh_price_exceeds_charge("03.50", "3.5"),
            "normalized equal"
        );
        assert!(!fresh_price_exceeds_charge("100", "99"));
        assert!(fresh_price_exceeds_charge("99", "100"));
        assert!(!fresh_price_exceeds_charge("0.5", "0.45"));
        assert!(fresh_price_exceeds_charge("0.45", "0.5"));
        assert!(fresh_price_exceeds_charge("0.4", "0.45"));
        assert!(!fresh_price_exceeds_charge("2", "2.000"));
    }

    #[test]
    fn worker_refuses_to_pay_for_a_zero_charged_line() {
        let refuses = |credits: &str, pay_wei: &str| {
            payment_is_positive(pay_wei) && !charge_is_positive(credits)
        };
        assert!(
            refuses("0", "10000000000000000"),
            "charged 0, pays >0 → refuse"
        );
        assert!(refuses("0.00", "1"), "charged 0.00, pays >0 → refuse");
        assert!(!refuses("3", "10000000000000000"), "charged >0 → proceed");
        assert!(!refuses("0", "0"), "free mint: pays 0 → proceed");
    }
}
