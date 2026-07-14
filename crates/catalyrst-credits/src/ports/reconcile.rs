use std::collections::HashSet;

use serde::Serialize;
use sqlx::Row;

use crate::http::ApiError;
use crate::ports::credits::CreditsComponent;

#[derive(Debug, Serialize)]
pub struct LedgerBalanceDiff {
    pub address: String,

    #[serde(rename = "ledgerSum")]
    pub ledger_sum: String,

    pub available: String,
}

#[derive(Debug, Serialize)]
pub struct PurchaseGrantDiff {
    pub address: String,
    #[serde(rename = "paidCredits")]
    pub paid_credits: String,
    #[serde(rename = "grantedCredits")]
    pub granted_credits: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutFulfillmentDiff {
    #[serde(rename = "checkoutId")]
    pub checkout_id: i64,
    #[serde(rename = "totalCredits")]
    pub total_credits: String,
    #[serde(rename = "confirmedSum")]
    pub confirmed_sum: String,
}

#[derive(Debug, Serialize)]
pub struct EscrowHoldings {
    pub available: bool,
    pub active: i64,
    pub revoked: i64,
    pub released: i64,
}

#[derive(Debug, Serialize)]
pub struct EscrowGrantDiff {
    #[serde(rename = "checkoutId")]
    pub checkout_id: i64,
    #[serde(rename = "outboxId")]
    pub outbox_id: i64,
    pub address: String,
    pub urn: String,
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    pub collection: Option<String>,
    #[serde(rename = "escrowRef")]
    pub escrow_ref: String,
    #[serde(rename = "unitPriceCredits")]
    pub unit_price_credits: String,
}

#[derive(Debug, Serialize)]
pub struct ReconcileReport {
    pub ok: bool,
    #[serde(rename = "ledgerBalanceMismatches")]
    pub ledger_balance_mismatches: Vec<LedgerBalanceDiff>,
    #[serde(rename = "earnedBalanceMismatches")]
    pub earned_balance_mismatches: Vec<LedgerBalanceDiff>,
    #[serde(rename = "purchaseGrantMismatches")]
    pub purchase_grant_mismatches: Vec<PurchaseGrantDiff>,
    #[serde(rename = "checkoutFulfillmentMismatches")]
    pub checkout_fulfillment_mismatches: Vec<CheckoutFulfillmentDiff>,
    #[serde(rename = "escrowHoldings")]
    pub escrow_holdings: EscrowHoldings,
    #[serde(rename = "escrowGrantMismatches")]
    pub escrow_grant_mismatches: Vec<EscrowGrantDiff>,
}

const RECONCILE_LIMIT: i64 = 500;

/// Wallets locked per transaction by
/// [`CreditsComponent::realign_ledger_to_balances`]. Small enough that the
/// exclusive lock it takes on the money path is short, large enough that the
/// walk is a handful of round trips per thousand wallets.
const REALIGN_BATCH: i64 = 500;

/// `tx_ref` stamped on every adjustment row written by
/// [`CreditsComponent::realign_ledger_to_balances`] and by migration 0018, so
/// the adjustments are greppable and separable from real money movement.
pub const REALIGN_TX_REF: &str = "migration:0018-ledger-realign";

/// One adjustment row written to make the ledger reproduce a stored balance.
#[derive(Debug, Serialize)]
pub struct LedgerRealignment {
    pub address: String,
    pub bucket: String,
    pub kind: String,
    pub amount: String,
}

impl CreditsComponent {
    pub async fn reconcile(
        &self,
        usage_grants_pool: Option<&sqlx::PgPool>,
    ) -> Result<ReconcileReport, ApiError> {
        let ledger_balance_mismatches = self.reconcile_ledger_balance().await?;
        let earned_balance_mismatches = self.reconcile_earned_balance().await?;
        let purchase_grant_mismatches = self.reconcile_purchase_grant().await?;
        let checkout_fulfillment_mismatches = self.reconcile_checkout_fulfillment().await?;
        let escrow_holdings = reconcile_escrow_holdings(usage_grants_pool).await?;
        let escrow_grant_mismatches = self.reconcile_escrow_grants(usage_grants_pool).await?;

        if !ledger_balance_mismatches.is_empty() {
            tracing::error!(
                invariant = "ledger_sum==available",
                count = ledger_balance_mismatches.len(),
                addresses = ?ledger_balance_mismatches
                    .iter()
                    .map(|d| d.address.as_str())
                    .collect::<Vec<_>>(),
                "RECONCILE ALERT: signed credit_ledger sum does not equal user_credits.available"
            );
        }
        if !earned_balance_mismatches.is_empty() {
            tracing::error!(
                invariant = "earned_ledger_sum==earned_available",
                count = earned_balance_mismatches.len(),
                addresses = ?earned_balance_mismatches
                    .iter()
                    .map(|d| d.address.as_str())
                    .collect::<Vec<_>>(),
                "RECONCILE ALERT: signed earned-bucket ledger sum does not equal \
                 user_credits.earned_available"
            );
        }
        if !purchase_grant_mismatches.is_empty() {
            tracing::error!(
                invariant = "paid_purchases==purchase_ledger",
                count = purchase_grant_mismatches.len(),
                "RECONCILE ALERT: paid Stripe purchases do not equal granted 'purchase' Credits"
            );
        }
        if !checkout_fulfillment_mismatches.is_empty() {
            tracing::error!(
                invariant = "checkout_total==confirmed_sum",
                count = checkout_fulfillment_mismatches.len(),
                "RECONCILE ALERT: fulfilled checkout totals do not equal confirmed line sums"
            );
        }
        if !escrow_grant_mismatches.is_empty() {
            tracing::error!(
                invariant = "confirmed_line==usage_grant",
                count = escrow_grant_mismatches.len(),
                escrow_refs = ?escrow_grant_mismatches
                    .iter()
                    .map(|d| d.escrow_ref.as_str())
                    .collect::<Vec<_>>(),
                "RECONCILE ALERT: confirmed fulfilment lines (Credits spent, escrow minted) lack a \
                 usage_grant \u{2014} the lease overlay is missing and must be re-granted"
            );
        }

        let ok = ledger_balance_mismatches.is_empty()
            && earned_balance_mismatches.is_empty()
            && purchase_grant_mismatches.is_empty()
            && checkout_fulfillment_mismatches.is_empty()
            && escrow_grant_mismatches.is_empty();

        Ok(ReconcileReport {
            ok,
            ledger_balance_mismatches,
            earned_balance_mismatches,
            purchase_grant_mismatches,
            checkout_fulfillment_mismatches,
            escrow_holdings,
            escrow_grant_mismatches,
        })
    }

    /// Make the ledger reproduce the stored balances by writing an explicit
    /// adjustment row for every wallet whose signed ledger sum disagrees.
    ///
    /// `reconcile_earned_balance` assumes a ledger-faithful origin, which
    /// migration 0012 does not provide: it seeded `earned_available` with
    /// `GREATEST(0, LEAST(available, earned_sum))`, and for any pre-existing
    /// wallet whose earned ledger sum fell outside `[0, available]` that clamp
    /// wrote a balance no ledger replay can reproduce. Those wallets would trip
    /// the reconcile alarm forever, through no fault of any later write.
    ///
    /// DECISION: record the clamp, do not tolerate it. Teaching reconcile to
    /// forgive a "clamped origin" turns an equality invariant into an equality
    /// with a permanent carve-out -- it stops being an invariant, it has to
    /// carry knowledge of a one-time migration forever, and the same tolerance
    /// would mask genuine divergence (the f64-vs-NUMERIC ledger-row drop fixed
    /// alongside this produced exactly the kind of gap it would swallow). An
    /// adjustment row is self-describing, auditable, and leaves reconcile a
    /// pure equality.
    ///
    /// Idempotent: a second call finds no divergence and writes nothing.
    /// Migration 0018 applies this once to historical data; this entry point
    /// exists so the same operation is testable and available to operators.
    ///
    /// BATCHED, and that is load-bearing. An earlier revision ran the whole
    /// thing as ONE transaction that took `FOR UPDATE` over the ENTIRE
    /// `user_credits` table -- twice -- with a correlated per-wallet aggregate
    /// over `credit_ledger` and no LIMIT. On a real wallet count that is a long
    /// exclusive lock across every wallet on the money path: `spend`, `refund`,
    /// `revoke` and checkout all block on the same rows. This walks the primary
    /// key in ordered keyset chunks of [`REALIGN_BATCH`] wallets, each chunk in
    /// its OWN transaction, so the widest lock it ever holds is one chunk and
    /// the operation is resumable: a chunk either commits or is retried, and a
    /// rerun simply finds no divergence in the chunks that already landed.
    ///
    /// Wallets created below the cursor while the walk is in flight are not
    /// seen by that run. That is safe (a newly created wallet is
    /// ledger-faithful by construction) and, since the operation is idempotent,
    /// a rerun picks up anything a run missed.
    ///
    /// TODO(owner-decision): scope. This realigns EVERY divergent wallet, not
    /// only those the 0012 clamp touched -- the two are indistinguishable after
    /// the fact, since neither leaves a marker. That is the safe direction (the
    /// balance is authoritative, and the adjustment rows name themselves), but
    /// it means migration 0018 also absorbs any divergence the f64-vs-NUMERIC
    /// ledger-row drop had already caused in production. Review the
    /// `credits.ledger_realign` audit rows after deploying and confirm the
    /// per-wallet deltas are the expected magnitude before treating the
    /// reconcile report as clean.
    pub async fn realign_ledger_to_balances(
        &self,
        reason: &str,
    ) -> Result<Vec<LedgerRealignment>, ApiError> {
        self.realign_ledger_to_balances_in_batches(reason, REALIGN_BATCH)
            .await
    }

    /// [`Self::realign_ledger_to_balances`] with an explicit chunk size. Exists
    /// so tests can drive the multi-chunk path without seeding thousands of
    /// wallets; operators should call the default entry point.
    pub async fn realign_ledger_to_balances_in_batches(
        &self,
        reason: &str,
        batch: i64,
    ) -> Result<Vec<LedgerRealignment>, ApiError> {
        let batch = batch.max(1);
        // The empty string precedes every address under every collation, so it
        // is the natural "before the first row" cursor for the keyset walk.
        let mut cursor = String::new();
        let mut out: Vec<LedgerRealignment> = Vec::new();
        let mut wallets_scanned: u64 = 0;
        let mut chunks: u64 = 0;

        loop {
            let mut tx = self.pool.begin().await?;

            // One bounded chunk of the PRIMARY KEY, locked for the duration of
            // this chunk's transaction only. `ORDER BY ... LIMIT ... FOR
            // UPDATE` locks after the limit, so exactly `batch` wallet rows are
            // held, never the whole table.
            let addresses: Vec<String> = sqlx::query_scalar(
                "SELECT address FROM user_credits \
                 WHERE address > $1 ORDER BY address LIMIT $2 FOR UPDATE",
            )
            .bind(&cursor)
            .bind(batch)
            .fetch_all(&mut *tx)
            .await?;

            if addresses.is_empty() {
                tx.rollback().await?;
                break;
            }
            cursor = addresses[addresses.len() - 1].clone();
            wallets_scanned += addresses.len() as u64;
            chunks += 1;

            // Earned bucket first, then the total, so the second adjustment
            // sees the first and the two compose instead of fighting. Both are
            // restricted to this chunk's already-locked addresses, so the
            // correlated per-wallet aggregate over `credit_ledger` runs `batch`
            // times rather than once per wallet in the table.
            let earned = sqlx::query(
                "WITH e AS ( \
                     SELECT u.address, \
                            u.earned_available - COALESCE(( \
                                SELECT SUM(CASE \
                                    WHEN l.kind IN ('grant','refund','purchase','claim') \
                                        THEN l.amount \
                                    WHEN l.kind IN ('spend','consume','expire') THEN -l.amount \
                                    ELSE 0 END) \
                                FROM credit_ledger l \
                                WHERE l.address = u.address AND l.bucket = 'earned' \
                            ), 0) AS delta \
                     FROM user_credits u WHERE u.address = ANY($2) \
                 ) \
                 INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
                 SELECT address, \
                        CASE WHEN delta > 0 THEN 'grant' ELSE 'consume' END, \
                        abs(delta), $1, 'earned', FALSE \
                 FROM e WHERE delta <> 0 \
                 RETURNING address, amount::text AS amount, kind",
            )
            .bind(REALIGN_TX_REF)
            .bind(&addresses)
            .fetch_all(&mut *tx)
            .await?;

            let total = sqlx::query(
                "WITH t AS ( \
                     SELECT u.address, \
                            u.available - COALESCE(( \
                                SELECT SUM(CASE \
                                    WHEN l.kind IN ('grant','refund','purchase','claim') \
                                        THEN l.amount \
                                    WHEN l.kind IN ('spend','consume','expire') THEN -l.amount \
                                    ELSE 0 END) \
                                FROM credit_ledger l \
                                WHERE l.address = u.address \
                            ), 0) AS delta \
                     FROM user_credits u WHERE u.address = ANY($2) \
                 ) \
                 INSERT INTO credit_ledger (address, kind, amount, tx_ref, bucket, captcha_ok) \
                 SELECT address, \
                        CASE WHEN delta > 0 THEN 'grant' ELSE 'consume' END, \
                        abs(delta), $1, 'paid', FALSE \
                 FROM t WHERE delta <> 0 \
                 RETURNING address, amount::text AS amount, kind",
            )
            .bind(REALIGN_TX_REF)
            .bind(&addresses)
            .fetch_all(&mut *tx)
            .await?;

            let mut applied_here = 0usize;
            for (rows, bucket) in [(earned, "earned"), (total, "paid")] {
                for r in rows {
                    let kind: String = r.get("kind");
                    let amount: String = r.get("amount");
                    let address: String = r.get("address");
                    Self::audit(
                        &mut *tx,
                        "credits.ledger_realign",
                        Some(&address),
                        None,
                        Some(&amount),
                        Some(reason),
                        Some("system"),
                        &serde_json::json!({
                            "bucket": bucket, "kind": kind, "txRef": REALIGN_TX_REF,
                        }),
                    )
                    .await?;
                    applied_here += 1;
                    out.push(LedgerRealignment {
                        address,
                        bucket: bucket.to_string(),
                        kind,
                        amount,
                    });
                }
            }

            // Commit per chunk: the adjustments so far are durable, and a
            // failure later resumes from here rather than redoing everything.
            tx.commit().await?;

            tracing::info!(
                chunk = chunks,
                wallets_scanned,
                adjustments_this_chunk = applied_here,
                adjustments_total = out.len(),
                cursor = %cursor,
                "ledger realign progress"
            );

            if (addresses.len() as i64) < batch {
                break;
            }
        }

        if out.is_empty() {
            tracing::info!(
                wallets_scanned,
                chunks,
                reason = %reason,
                "ledger realign found no divergence; nothing written"
            );
        } else {
            tracing::warn!(
                count = out.len(),
                wallets_scanned,
                chunks,
                reason = %reason,
                "ledger realigned to stored balances: adjustment rows written so replay \
                 reproduces the balance"
            );
        }
        Ok(out)
    }

    async fn reconcile_earned_balance(&self) -> Result<Vec<LedgerBalanceDiff>, ApiError> {
        let rows = sqlx::query(
            "WITH ledger AS ( \
                 SELECT address, \
                        SUM(CASE \
                                WHEN kind IN ('grant','refund','purchase','claim') THEN amount \
                                WHEN kind IN ('spend','consume','expire') THEN -amount \
                                ELSE 0 END) AS s \
                 FROM credit_ledger WHERE bucket = 'earned' GROUP BY address \
             ) \
             SELECT COALESCE(l.address, u.address) AS address, \
                    COALESCE(l.s, 0)::text AS ledger_sum, \
                    COALESCE(u.earned_available, 0)::text AS available \
             FROM ledger l \
             FULL OUTER JOIN user_credits u ON u.address = l.address \
             WHERE COALESCE(l.s, 0) <> COALESCE(u.earned_available, 0) \
             ORDER BY 1 LIMIT $1",
        )
        .bind(RECONCILE_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| LedgerBalanceDiff {
                address: r.get("address"),
                ledger_sum: r.get("ledger_sum"),
                available: r.get("available"),
            })
            .collect())
    }

    async fn reconcile_ledger_balance(&self) -> Result<Vec<LedgerBalanceDiff>, ApiError> {
        let rows = sqlx::query(
            "WITH ledger AS ( \
                 SELECT address, \
                        SUM(CASE \
                                WHEN kind IN ('grant','refund','purchase','claim') THEN amount \
                                WHEN kind IN ('spend','consume','expire') THEN -amount \
                                ELSE 0 END) AS s \
                 FROM credit_ledger GROUP BY address \
             ) \
             SELECT COALESCE(l.address, u.address) AS address, \
                    COALESCE(l.s, 0)::text AS ledger_sum, \
                    COALESCE(u.available, 0)::text AS available \
             FROM ledger l \
             FULL OUTER JOIN user_credits u ON u.address = l.address \
             WHERE COALESCE(l.s, 0) <> COALESCE(u.available, 0) \
             ORDER BY 1 LIMIT $1",
        )
        .bind(RECONCILE_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| LedgerBalanceDiff {
                address: r.get("address"),
                ledger_sum: r.get("ledger_sum"),
                available: r.get("available"),
            })
            .collect())
    }

    async fn reconcile_purchase_grant(&self) -> Result<Vec<PurchaseGrantDiff>, ApiError> {
        let rows = sqlx::query(
            "WITH paid AS ( \
                 SELECT address, SUM(credits) AS c \
                 FROM credit_purchases \
                 WHERE status IN ('paid','refunded','disputed') GROUP BY address \
             ), granted AS ( \
                 SELECT address, SUM(amount) AS g \
                 FROM credit_ledger WHERE kind = 'purchase' GROUP BY address \
             ) \
             SELECT COALESCE(p.address, g.address) AS address, \
                    COALESCE(p.c, 0)::text AS paid_credits, \
                    COALESCE(g.g, 0)::text AS granted_credits \
             FROM paid p \
             FULL OUTER JOIN granted g ON g.address = p.address \
             WHERE COALESCE(p.c, 0) <> COALESCE(g.g, 0) \
             ORDER BY 1 LIMIT $1",
        )
        .bind(RECONCILE_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PurchaseGrantDiff {
                address: r.get("address"),
                paid_credits: r.get("paid_credits"),
                granted_credits: r.get("granted_credits"),
            })
            .collect())
    }

    async fn reconcile_checkout_fulfillment(
        &self,
    ) -> Result<Vec<CheckoutFulfillmentDiff>, ApiError> {
        let rows = sqlx::query(
            "SELECT c.id AS id, \
                    c.total_credits::text AS total_credits, \
                    COALESCE(o.s, 0)::text AS confirmed_sum \
             FROM checkouts c \
             LEFT JOIN ( \
                 SELECT checkout_id, SUM(unit_price_credits) AS s \
                 FROM fulfillment_outbox WHERE status = 'confirmed' GROUP BY checkout_id \
             ) o ON o.checkout_id = c.id \
             WHERE c.status = 'fulfilled' AND c.total_credits <> COALESCE(o.s, 0) \
             ORDER BY c.id LIMIT $1",
        )
        .bind(RECONCILE_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CheckoutFulfillmentDiff {
                checkout_id: r.get("id"),
                total_credits: r.get("total_credits"),
                confirmed_sum: r.get("confirmed_sum"),
            })
            .collect())
    }

    async fn reconcile_escrow_grants(
        &self,
        usage_grants_pool: Option<&sqlx::PgPool>,
    ) -> Result<Vec<EscrowGrantDiff>, ApiError> {
        let Some(ug_pool) = usage_grants_pool else {
            return Ok(Vec::new());
        };

        let confirmed = sqlx::query(
            "SELECT o.id AS outbox_id, o.checkout_id, c.address AS address, o.urn, \
                    o.token_id, \
                    COALESCE(o.collection, lower(split_part(o.urn, ':', 5))) AS collection, \
                    o.external_ref, \
                    o.unit_price_credits::text AS unit_price_credits \
             FROM fulfillment_outbox o \
             JOIN checkouts c ON c.id = o.checkout_id \
             WHERE o.status = 'confirmed' AND o.external_ref IS NOT NULL \
             ORDER BY o.id LIMIT $1",
        )
        .bind(RECONCILE_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        if confirmed.is_empty() {
            return Ok(Vec::new());
        }

        let refs: Vec<String> = {
            let mut seen = HashSet::new();
            confirmed
                .iter()
                .filter_map(|r| {
                    let er: String = r.get("external_ref");
                    seen.insert(er.clone()).then_some(er)
                })
                .collect()
        };

        let existing: HashSet<String> = sqlx::query(
            "SELECT DISTINCT escrow_ref FROM marketplace.usage_grants \
             WHERE escrow_ref = ANY($1)",
        )
        .bind(&refs)
        .fetch_all(ug_pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("escrow_ref"))
        .collect();

        Ok(confirmed
            .into_iter()
            .filter_map(|r| {
                let escrow_ref: String = r.get("external_ref");
                if existing.contains(&escrow_ref) {
                    return None;
                }
                Some(EscrowGrantDiff {
                    checkout_id: r.get("checkout_id"),
                    outbox_id: r.get("outbox_id"),
                    address: r.get("address"),
                    urn: r.get("urn"),
                    token_id: r.get("token_id"),
                    collection: r.get("collection"),
                    escrow_ref,
                    unit_price_credits: r.get("unit_price_credits"),
                })
            })
            .collect())
    }
}

async fn reconcile_escrow_holdings(
    usage_grants_pool: Option<&sqlx::PgPool>,
) -> Result<EscrowHoldings, ApiError> {
    let Some(pool) = usage_grants_pool else {
        return Ok(EscrowHoldings {
            available: false,
            active: 0,
            revoked: 0,
            released: 0,
        });
    };
    let rows = sqlx::query(
        "SELECT status, count(*)::bigint AS n \
         FROM marketplace.usage_grants GROUP BY status",
    )
    .fetch_all(pool)
    .await?;
    let mut holdings = EscrowHoldings {
        available: true,
        active: 0,
        revoked: 0,
        released: 0,
    };
    for r in rows {
        let status: String = r.get("status");
        let n: i64 = r.get("n");
        match status.as_str() {
            "active" => holdings.active = n,
            "revoked" => holdings.revoked = n,
            "released" => holdings.released = n,
            _ => {}
        }
    }
    Ok(holdings)
}
