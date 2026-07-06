use std::time::Duration;

use sqlx::Row;

use crate::ports::broker::PurchaseMode;
use crate::ports::signer::{DirectSigner, ReceiptOutcome};
use crate::AppState;

pub fn next_broker_status(
    current: &str,
    mode: PurchaseMode,
    outcome: ReceiptOutcome,
) -> Option<&'static str> {
    match (current, outcome) {
        ("sent", ReceiptOutcome::Confirmed) if mode.has_forward_leg() => Some("bought"),
        ("sent", ReceiptOutcome::Confirmed) => Some("confirmed"),
        ("sent", ReceiptOutcome::Reverted) => Some("reverted"),
        ("forwarding", ReceiptOutcome::Confirmed) => Some("confirmed"),
        ("forwarding", ReceiptOutcome::Reverted) => Some("reverted"),
        _ => None,
    }
}

fn pollable_hash(
    status: &str,
    tx_hash: Option<&str>,
    forward_tx_hash: Option<&str>,
) -> Option<String> {
    match status {
        "sent" => tx_hash.map(str::to_string),
        "forwarding" => forward_tx_hash.map(str::to_string),
        _ => None,
    }
}

pub async fn reconcile_once(state: &AppState, signer: &DirectSigner) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT idempotency_key, status, mode, tx_hash, forward_tx_hash \
         FROM broker_purchases \
         WHERE status IN ('sent', 'forwarding') \
           AND chain_id = $1 \
           AND updated_at < NOW() - INTERVAL '2 minutes' \
         ORDER BY updated_at ASC \
         LIMIT 50",
    )
    .bind(signer.chain_id() as i64)
    .fetch_all(&state.pool)
    .await?;

    let mut advanced = 0u64;
    for r in rows {
        let key: Option<String> = r.get("idempotency_key");
        let status: String = r.get("status");
        let mode_raw: String = r.get("mode");
        let tx_hash: Option<String> = r.get("tx_hash");
        let forward_tx_hash: Option<String> = r.get("forward_tx_hash");

        let Some(key) = key else { continue };
        let Some(mode) = PurchaseMode::from_db_str(&mode_raw) else {
            tracing::warn!(idempotency_key = %key, mode = %mode_raw, "reconcile: unknown purchase mode; skipping row");
            continue;
        };
        let Some(hash) = pollable_hash(&status, tx_hash.as_deref(), forward_tx_hash.as_deref())
        else {
            continue;
        };

        let outcome = match signer.await_receipt(&hash).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(idempotency_key = %key, error = %e, "reconcile: receipt poll failed; will retry next tick");
                continue;
            }
        };

        if let Some(new_status) = next_broker_status(&status, mode, outcome) {
            sqlx::query(
                "UPDATE broker_purchases SET status = $2, updated_at = NOW() \
                 WHERE idempotency_key = $1 AND status = $3",
            )
            .bind(&key)
            .bind(new_status)
            .bind(&status)
            .execute(&state.pool)
            .await?;
            advanced += 1;
            tracing::info!(
                idempotency_key = %key,
                from = %status,
                to = %new_status,
                tx_hash = %hash,
                "reconcile: broker buy advanced from receipt"
            );
        }
    }
    Ok(advanced)
}

pub async fn reconcile_name_transfers_once(
    state: &AppState,
    signer: &DirectSigner,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT idempotency_key, tx_hash \
         FROM name_transfers \
         WHERE status = 'sent' \
           AND chain_id = $1 \
           AND updated_at < NOW() - INTERVAL '2 minutes' \
         ORDER BY updated_at ASC \
         LIMIT 50",
    )
    .bind(signer.chain_id() as i64)
    .fetch_all(&state.pool)
    .await?;

    let mut advanced = 0u64;
    for r in rows {
        let key: String = r.get("idempotency_key");
        let hash: Option<String> = r.get("tx_hash");
        let Some(hash) = hash else { continue };

        let outcome = match signer.await_receipt(&hash).await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(idempotency_key = %key, error = %e, "reconcile: name transfer receipt poll failed; will retry next tick");
                continue;
            }
        };
        let new_status = match outcome {
            ReceiptOutcome::Confirmed => "confirmed",
            ReceiptOutcome::Reverted => "reverted",
            ReceiptOutcome::Pending => continue,
        };

        sqlx::query(
            "UPDATE name_transfers SET status = $2, updated_at = NOW() \
             WHERE idempotency_key = $1 AND status = 'sent'",
        )
        .bind(&key)
        .bind(new_status)
        .execute(&state.pool)
        .await?;
        advanced += 1;
        tracing::info!(
            idempotency_key = %key,
            to = %new_status,
            tx_hash = %hash,
            "reconcile: name transfer settled from receipt"
        );
    }
    Ok(advanced)
}

pub fn spawn_broker_reconciler(state: AppState, interval: Duration) {
    if !state.transaction.has_direct_signer() && state.eth_signer.is_none() {
        tracing::info!(
            "broker reconciler not started (no direct JSON-RPC signer; nothing to poll receipts with)"
        );
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let polygon = async {
                match state.transaction.direct_signer() {
                    Some(signer) => reconcile_once(&state, signer).await.unwrap_or_else(|e| {
                        tracing::error!(error = %e, chain_id = signer.chain_id(), "broker reconcile pass failed");
                        0
                    }),
                    None => 0,
                }
            };
            let ethereum = async {
                match state.eth_signer.as_ref() {
                    Some(signer) => {
                        let buys = reconcile_once(&state, signer).await.unwrap_or_else(|e| {
                            tracing::error!(error = %e, chain_id = signer.chain_id(), "broker reconcile pass failed");
                            0
                        });
                        let transfers = reconcile_name_transfers_once(&state, signer)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::error!(error = %e, "name transfer reconcile pass failed");
                                0
                            });
                        buys + transfers
                    }
                    None => 0,
                }
            };
            let (a, b) = tokio::join!(polygon, ethereum);
            if a + b > 0 {
                tracing::info!(advanced = a + b, "broker reconcile pass");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transitions_are_correct() {
        use PurchaseMode::{Primary, Secondary};
        assert_eq!(
            next_broker_status("sent", Secondary, ReceiptOutcome::Confirmed),
            Some("bought")
        );
        assert_eq!(
            next_broker_status("sent", Primary, ReceiptOutcome::Reverted),
            Some("reverted")
        );
        assert_eq!(
            next_broker_status("forwarding", Primary, ReceiptOutcome::Confirmed),
            Some("confirmed")
        );
        assert_eq!(
            next_broker_status("forwarding", Secondary, ReceiptOutcome::Reverted),
            Some("reverted")
        );
        assert_eq!(
            next_broker_status("sent", Primary, ReceiptOutcome::Pending),
            None
        );
        assert_eq!(
            next_broker_status("forwarding", Secondary, ReceiptOutcome::Pending),
            None
        );
        assert_eq!(
            next_broker_status("confirmed", Secondary, ReceiptOutcome::Confirmed),
            None
        );
        assert_eq!(
            next_broker_status("bought", Primary, ReceiptOutcome::Confirmed),
            None
        );
    }

    #[test]
    fn name_modes_confirm_straight_from_sent() {
        use PurchaseMode::{NameMint, NameSecondary};
        assert_eq!(
            next_broker_status("sent", NameMint, ReceiptOutcome::Confirmed),
            Some("confirmed")
        );
        assert_eq!(
            next_broker_status("sent", NameSecondary, ReceiptOutcome::Confirmed),
            Some("confirmed")
        );
        assert_eq!(
            next_broker_status("sent", NameMint, ReceiptOutcome::Reverted),
            Some("reverted")
        );
        assert_eq!(
            next_broker_status("sent", NameSecondary, ReceiptOutcome::Pending),
            None
        );
    }

    #[test]
    fn pollable_hash_picks_the_right_tx() {
        assert_eq!(
            pollable_hash("sent", Some("0xbuy"), Some("0xfwd")),
            Some("0xbuy".to_string())
        );
        assert_eq!(
            pollable_hash("forwarding", Some("0xbuy"), Some("0xfwd")),
            Some("0xfwd".to_string())
        );
        assert_eq!(pollable_hash("sent", None, Some("0xfwd")), None);
        assert_eq!(pollable_hash("bought", Some("0xbuy"), None), None);
    }
}
