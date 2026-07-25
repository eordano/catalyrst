use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value as JsonValue};

use crate::http::ApiError;
use crate::ports::packs::{MarkPaidOutcome, RefundOutcome};
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

    match state
        .credits
        .record_charge_refund(pi_id, cumulative_refunded_cents, event_id)
        .await?
    {
        RefundOutcome::Refund { address, credits } => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                address = %address,
                credits = %credits,
                "charge.refunded: refunded credits (atomic)"
            );
        }
        RefundOutcome::NothingToRefund => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                "charge.refunded added no new refunded amount; no credits refunded"
            );
        }
        RefundOutcome::NoPaidPurchase => {
            tracing::warn!(
                event_id = %event_id,
                payment_intent = %pi_id,
                "charge.refunded matched no paid purchase; no refund"
            );
        }
    }
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

    match state
        .credits
        .record_full_reversal(pi_id, status, event_id)
        .await?
    {
        RefundOutcome::Refund { address, credits } => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                status = %status,
                address = %address,
                credits = %credits,
                "reversal: refunded remaining credits (atomic)"
            );
        }
        RefundOutcome::NothingToRefund => {
            tracing::info!(
                event_id = %event_id,
                payment_intent = %pi_id,
                status = %status,
                "reversal: nothing left un-refunded; no credits refunded"
            );
        }
        RefundOutcome::NoPaidPurchase => {
            tracing::warn!(
                event_id = %event_id,
                payment_intent = %pi_id,
                status = %status,
                "reversal matched no paid purchase; no refund"
            );
        }
    }
    Ok(())
}
