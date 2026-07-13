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
                 usage_grant — the lease overlay is missing and must be re-granted"
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
