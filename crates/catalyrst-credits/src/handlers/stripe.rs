use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value as JsonValue};

use crate::http::ApiError;
use crate::ports::packs::{MarkPaidOutcome, ReversalOutcome};
use crate::ports::stripe::{verify_stripe_signature, SIGNATURE_TOLERANCE_SECS};
use crate::AppState;

fn ok(detail: &str) -> Json<JsonValue> {
    Json(json!({ "ok": true, "detail": detail }))
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,

    body: Result<Bytes, BytesRejection>,
) -> Result<Json<JsonValue>, ApiError> {
    let body = body.map_err(|e| ApiError::bad_request(e.body_text()))?;

    let Some(secret) = state.stripe_webhook_secret.as_ref() else {
        return Err(ApiError::not_implemented(
            "stripe webhook disabled (STRIPE_WEBHOOK_SECRET unset)",
        ));
    };

    let sig_header = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad_request("missing Stripe-Signature header"))?;

    let now = chrono::Utc::now().timestamp();
    if !verify_stripe_signature(secret, sig_header, &body, SIGNATURE_TOLERANCE_SECS, now) {
        return Err(ApiError::bad_request("invalid Stripe signature"));
    }

    let event: JsonValue =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("invalid webhook body"))?;

    let event_id = event
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("event missing id"))?
        .to_string();
    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("event missing type"))?
        .to_string();

    let needs_processing = state
        .credits
        .record_stripe_event(&event_id, &event_type, &event)
        .await?;
    if !needs_processing {
        return Ok(ok("already processed"));
    }

    let object = event
        .get("data")
        .and_then(|d| d.get("object"))
        .cloned()
        .unwrap_or(JsonValue::Null);

    match event_type.as_str() {
        "payment_intent.succeeded" => {
            let pi_id = object
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("payment_intent missing id"))?;

            let charged_cents = object
                .get("amount")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ApiError::bad_request("payment_intent missing amount"))?;

            match state
                .credits
                .mark_purchase_paid(pi_id, &event_id, charged_cents)
                .await?
            {
                MarkPaidOutcome::Granted { address, credits } => {
                    let detail = json!({
                        "source": "stripe",
                        "eventId": event_id,
                        "paymentIntent": pi_id,
                        "sku": object.get("metadata").and_then(|m| m.get("sku")),
                    });

                    state
                        .credits
                        .admin_grant_credits(
                            &address,
                            &credits,
                            "purchase",
                            Some("stripe purchase"),
                            Some("stripe"),
                            Some(&event_id),
                            &detail,
                        )
                        .await?;
                }
                MarkPaidOutcome::AmountMismatch {
                    expected_cents,
                    charged_cents,
                } => {
                    tracing::warn!(
                        event_id = %event_id,
                        payment_intent = %pi_id,
                        expected_cents,
                        charged_cents,
                        "payment_intent.succeeded amount mismatch; NOT granting credits"
                    );
                }
                MarkPaidOutcome::NoPendingPurchase => {
                    tracing::warn!(
                        event_id = %event_id,
                        payment_intent = %pi_id,
                        "payment_intent.succeeded matched no pending purchase; no grant"
                    );
                }
            }
        }
        "checkout.session.completed" => {
            let session_id = object
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("checkout.session missing id"))?;

            let charged_cents = object
                .get("amount_total")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ApiError::bad_request("checkout.session missing amount_total"))?;

            match state
                .credits
                .mark_order_paid_by_session(session_id, &event_id, charged_cents)
                .await?
            {
                MarkPaidOutcome::Granted { address, credits } => {
                    let detail = json!({
                        "source": "stripe",
                        "eventId": event_id,
                        "checkoutSession": session_id,
                        "sku": object.get("metadata").and_then(|m| m.get("sku")),
                        "orderId": object.get("metadata").and_then(|m| m.get("orderId")),
                    });

                    state
                        .credits
                        .admin_grant_credits(
                            &address,
                            &credits,
                            "purchase",
                            Some("stripe checkout purchase"),
                            Some("stripe"),
                            Some(&event_id),
                            &detail,
                        )
                        .await?;
                }
                MarkPaidOutcome::AmountMismatch {
                    expected_cents,
                    charged_cents,
                } => {
                    tracing::warn!(
                        event_id = %event_id,
                        checkout_session = %session_id,
                        expected_cents,
                        charged_cents,
                        "checkout.session.completed amount mismatch; NOT granting credits"
                    );
                }
                MarkPaidOutcome::NoPendingPurchase => {
                    tracing::warn!(
                        event_id = %event_id,
                        checkout_session = %session_id,
                        "checkout.session.completed matched no pending order; no grant"
                    );
                }
            }
        }
        "checkout.session.expired" => {
            mark_session_terminal(&state, &object, &event_id, "abandoned", None).await?;
        }
        "checkout.session.async_payment_failed" => {
            let reason = object
                .get("last_payment_error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str());
            mark_session_terminal(&state, &object, &event_id, "failed", reason).await?;
        }
        "charge.refunded" => {
            apply_partial_refund(&state, &object, &event_id).await?;
        }
        "charge.dispute.created" | "charge.dispute.funds_withdrawn" => {
            apply_full_reversal(&state, &object, &event_id, "disputed").await?;
        }
        _ => {}
    }

    state.credits.mark_stripe_event_processed(&event_id).await?;
    Ok(ok("processed"))
}

async fn mark_session_terminal(
    state: &AppState,
    object: &JsonValue,
    event_id: &str,
    status: &str,
    failure_reason: Option<&str>,
) -> Result<(), ApiError> {
    let Some(session_id) = object.get("id").and_then(|v| v.as_str()) else {
        tracing::warn!(event_id = %event_id, status = %status, "checkout.session event missing id");
        return Ok(());
    };
    let changed = state
        .credits
        .mark_order_status_by_session(session_id, status, failure_reason)
        .await?;
    if !changed {
        tracing::info!(
            event_id = %event_id,
            checkout_session = %session_id,
            status = %status,
            "checkout.session terminal event matched no pending order"
        );
    }
    Ok(())
}

async fn apply_partial_refund(
    state: &AppState,
    object: &JsonValue,
    event_id: &str,
) -> Result<(), ApiError> {
    let Some(pi_id) = object.get("payment_intent").and_then(|v| v.as_str()) else {
        tracing::warn!(
            event_id = %event_id,
            "charge.refunded has no payment_intent; nothing to reverse"
        );
        return Ok(());
    };
    let cumulative_refunded_cents = object
        .get("amount_refunded")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("charge.refunded missing amount_refunded"))?;

    let outcome = state
        .credits
        .record_charge_refund(pi_id, cumulative_refunded_cents, event_id)
        .await?;
    log_reversal(&outcome, event_id, pi_id, "charge.refunded");
    Ok(())
}

async fn apply_full_reversal(
    state: &AppState,
    object: &JsonValue,
    event_id: &str,
    status: &str,
) -> Result<(), ApiError> {
    let Some(pi_id) = object.get("payment_intent").and_then(|v| v.as_str()) else {
        tracing::warn!(
            event_id = %event_id,
            status = %status,
            "reversal event has no payment_intent; nothing to reverse"
        );
        return Ok(());
    };

    let outcome = state
        .credits
        .record_full_reversal(pi_id, status, event_id)
        .await?;
    log_reversal(&outcome, event_id, pi_id, status);
    Ok(())
}

fn log_reversal(outcome: &ReversalOutcome, event_id: &str, pi_id: &str, kind: &str) {
    match outcome {
        ReversalOutcome::Reversed {
            address,
            charged_back,
            removed,
            shortfall,
            has_shortfall,
        } => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                kind = %kind,
                address = %address,
                charged_back = %charged_back,
                removed = %removed,
                "fiat reversed: revoked the credits this purchase granted (atomic)"
            );
            // The buyer had already spent part of what we are clawing back:
            // the fiat is gone AND the credits were consumed. Unrecoverable
            // without a manual write-off, so it must page someone.
            if *has_shortfall {
                tracing::error!(
                    event_id = %event_id,
                    payment_intent = %pi_id,
                    address = %address,
                    charged_back = %charged_back,
                    removed = %removed,
                    shortfall = %shortfall,
                    "REVERSAL SHORTFALL: buyer had already spent part of the reversed \
                     purchase's credits; that amount is an unrecovered loss"
                );
            }
        }
        ReversalOutcome::NothingToReverse => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                kind = %kind,
                "reversal added no new reversed amount; no credits revoked"
            );
        }
        ReversalOutcome::NoPaidPurchase => {
            tracing::warn!(
                event_id = %event_id,
                payment_intent = %pi_id,
                kind = %kind,
                "reversal matched no paid purchase; nothing revoked"
            );
        }
    }
}
