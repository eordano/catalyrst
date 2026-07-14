use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth_chain::AUTH_TIMESTAMP_HEADER;
use crate::handlers::{random_32_hex, signer_from};
use crate::http::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CheckoutRequestBody {
    #[serde(rename = "packId")]
    pack_id: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CheckoutSessionOut {
    #[serde(rename = "orderId")]
    pub order_id: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct CreditsOrderStatusOut {
    pub status: String,
    #[serde(rename = "creditsGranted")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub credits_granted: i64,
    #[serde(rename = "newBalance")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub new_balance: i64,
    pub error: String,
}

fn checkout_idempotency_key(
    signer: &str,
    pack: &crate::ports::packs::PackRow,
    auth_ts: &str,
) -> String {
    let mut h = Sha256::new();
    for field in [
        signer,
        pack.sku.as_str(),
        pack.credits.as_str(),
        &pack.price_cents.to_string(),
        pack.currency.as_str(),
        auth_ts,
    ] {
        h.update(field.as_bytes());
        h.update([0u8]);
    }
    format!("landiler-checkout-{}", hex::encode(h.finalize()))
}

pub async fn create_checkout(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<CheckoutSessionOut>, ApiError> {
    let signer = signer_from(&headers, "post", "/credits/checkout").await?;
    let body: CheckoutRequestBody =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("invalid request body"))?;
    let auth_ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let Some(stripe) = state.stripe.as_ref() else {
        return Err(ApiError::service_unavailable(
            "card purchases are unavailable (payments provider not configured)",
        ));
    };
    if state.checkout_success_url.is_empty() || state.checkout_cancel_url.is_empty() {
        return Err(ApiError::service_unavailable(
            "card purchases are unavailable (checkout redirect URLs not configured)",
        ));
    }

    let pack = state
        .credits
        .get_pack(&body.pack_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("unknown or inactive packId"))?;

    tracing::debug!(source = ?body.source, sku = %pack.sku, "credits checkout requested");

    let order_id = format!("ord_{}", random_32_hex());
    let idempotency_key = checkout_idempotency_key(signer.as_str(), &pack, auth_ts);

    let session = stripe
        .create_checkout_session(
            &pack.currency,
            pack.price_cents,
            &pack.title,
            &state.checkout_success_url,
            &state.checkout_cancel_url,
            signer.as_str(),
            &pack.sku,
            &pack.credits,
            &order_id,
            &idempotency_key,
        )
        .await?;

    state
        .credits
        .insert_pending_order(&order_id, signer.as_str(), &pack, &session.id)
        .await?;

    Ok(Json(CheckoutSessionOut {
        order_id,
        url: session.url,
    }))
}

fn wire_status(db_status: &str) -> &'static str {
    match db_status {
        "pending" => "processing",
        "paid" | "refunded" | "disputed" => "credited",
        "failed" => "failed",
        "abandoned" => "abandoned",
        _ => "processing",
    }
}

pub async fn order_status(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<CreditsOrderStatusOut>, ApiError> {
    let path = format!("/credits/orders/{}", order_id);
    let signer = signer_from(&headers, "get", &path).await?;

    let order = state
        .credits
        .get_order(&order_id, signer.as_str())
        .await?
        .ok_or_else(|| ApiError::not_found("order not found"))?;

    let status = wire_status(&order.status);

    let credits_granted = if status == "credited" {
        order.credits.parse::<f64>().unwrap_or(0.0).floor() as i64
    } else {
        0
    };

    let available = state
        .credits
        .user_credits(signer.as_str())
        .await?
        .map(|c| c.available)
        .unwrap_or(0.0);
    let new_balance = available.floor() as i64;

    let error = if status == "failed" {
        order.failure_reason.clone().unwrap_or_default()
    } else {
        String::new()
    };

    Ok(Json(CreditsOrderStatusOut {
        status: status.to_string(),
        credits_granted,
        new_balance,
        error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_checkout_session() {
        let out = CheckoutSessionOut {
            order_id: "ord_abc".into(),
            url: "https://checkout.example/session/cs_test".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({ "orderId": "ord_abc", "url": "https://checkout.example/session/cs_test" })
        );
    }

    #[test]
    fn wire_identity_order_status() {
        let out = CreditsOrderStatusOut {
            status: "credited".into(),
            credits_granted: 100,
            new_balance: 250,
            error: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "status": "credited",
                "creditsGranted": 100,
                "newBalance": 250,
                "error": "",
            })
        );
    }

    #[test]
    fn db_status_maps_to_the_lowercase_unity_vocabulary() {
        assert_eq!(wire_status("pending"), "processing");
        assert_eq!(wire_status("paid"), "credited");
        assert_eq!(wire_status("refunded"), "credited");
        assert_eq!(wire_status("disputed"), "credited");
        assert_eq!(wire_status("failed"), "failed");
        assert_eq!(wire_status("abandoned"), "abandoned");
    }

    #[test]
    fn checkout_key_is_deterministic_and_distinct_from_intent() {
        let pack = crate::ports::packs::PackRow {
            sku: "pack_100".into(),
            title: "100 Credits".into(),
            credits: "100".into(),
            price_cents: 999,
            currency: "usd".into(),
            sort_order: 0,
        };
        let k = checkout_idempotency_key("0xabc", &pack, "1690000000000");
        assert_eq!(k, checkout_idempotency_key("0xabc", &pack, "1690000000000"));
        assert!(k.starts_with("landiler-checkout-"));
    }
}
