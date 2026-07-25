use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth_chain::AUTH_TIMESTAMP_HEADER;
use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct PackOut {
    sku: String,
    title: String,

    credits: String,
    #[serde(rename = "priceCents")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    price_cents: i64,
    currency: String,
    #[serde(rename = "sortOrder")]
    sort_order: i32,
}

impl From<crate::ports::packs::PackRow> for PackOut {
    fn from(p: crate::ports::packs::PackRow) -> Self {
        PackOut {
            sku: p.sku,
            title: p.title,
            credits: p.credits,
            price_cents: p.price_cents,
            currency: p.currency,
            sort_order: p.sort_order,
        }
    }
}

pub async fn list_packs(State(state): State<AppState>) -> Result<Json<Vec<PackOut>>, ApiError> {
    let rows = state.credits.list_active_packs().await?;
    Ok(Json(rows.into_iter().map(PackOut::from).collect()))
}

fn validate_sku(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 100 {
        return Err(ApiError::bad_request("invalid sku"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid sku"));
    }
    Ok(s.to_string())
}

fn intent_idempotency_key(
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
    format!("landiler-intent-{}", hex::encode(h.finalize()))
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct PackIntentOut {
    #[serde(rename = "clientSecret")]
    client_secret: String,
    #[serde(rename = "paymentIntentId")]
    payment_intent_id: String,
}

pub async fn create_intent(
    State(state): State<AppState>,
    Path(sku): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PackIntentOut>, ApiError> {
    let sku = validate_sku(&sku)?;
    let path = format!("/packs/{}/intent", sku);
    let signer = signer_from(&headers, "post", &path)?;
    let auth_ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let Some(stripe) = state.stripe.as_ref() else {
        return Err(ApiError::not_implemented(
            "card purchases are disabled (STRIPE_SECRET_KEY unset)",
        ));
    };

    let pack = state
        .credits
        .get_pack(&sku)
        .await?
        .ok_or_else(|| ApiError::not_found("pack not found or inactive"))?;

    let idempotency_key = intent_idempotency_key(&signer, &pack, auth_ts);

    let pi = stripe
        .create_payment_intent(
            pack.price_cents,
            &pack.currency,
            &signer,
            &pack.sku,
            &pack.credits,
            &idempotency_key,
        )
        .await?;

    state
        .credits
        .insert_pending_purchase(&signer, &pack, &pi.id)
        .await?;

    Ok(Json(PackIntentOut {
        client_secret: pi.client_secret,
        payment_intent_id: pi.id,
    }))
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct MockPurchaseOut {
    #[serde(rename = "creditsGranted")]
    credits_granted: String,
    available: String,
    mock: bool,
}

pub async fn mock_purchase(
    State(state): State<AppState>,
    Path(sku): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MockPurchaseOut>, ApiError> {
    if !state.mock_card {
        return Err(ApiError::not_implemented(
            "mock card purchases are disabled (CREDITS_MOCK_CARD unset)",
        ));
    }
    let sku = validate_sku(&sku)?;
    let path = format!("/packs/{}/mock-purchase", sku);
    let signer = signer_from(&headers, "post", &path)?;
    let auth_ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let pack = state
        .credits
        .get_pack(&sku)
        .await?
        .ok_or_else(|| ApiError::not_found("pack not found or inactive"))?;

    let idem = format!("mock-card:{}:{}:{}", signer, pack.sku, auth_ts);
    let detail = serde_json::json!({
        "source": "card-mock",
        "sku": pack.sku,
        "priceCents": pack.price_cents,
        "mock": true,
    });
    let outcome = state
        .credits
        .admin_grant_credits(
            &signer,
            &pack.credits,
            "purchase",
            Some("mock card purchase (no real charge)"),
            Some("card-mock"),
            Some(&idem),
            &detail,
        )
        .await?;

    Ok(Json(MockPurchaseOut {
        credits_granted: pack.credits.clone(),
        available: outcome.available,
        mock: true,
    }))
}

pub const MOCK_TOPUP_MAX_CREDITS: u32 = 10_000;

fn exceeds_mock_topup_cap(amount: &str) -> bool {
    let (int_part, frac_part) = amount.split_once('.').unwrap_or((amount, ""));
    let int_digits = int_part.trim_start_matches('0');
    let cap = MOCK_TOPUP_MAX_CREDITS.to_string();
    match int_digits.len().cmp(&cap.len()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match int_digits.cmp(cap.as_str()) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => frac_part.bytes().any(|b| b != b'0'),
        },
    }
}

#[derive(Debug, Deserialize)]
pub struct MockTopupBody {
    credits: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
#[serde(rename_all = "camelCase")]
pub struct MockTopupOut {
    credits_granted: String,
    available: String,
    mock: bool,
}

pub async fn mock_topup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MockTopupBody>,
) -> Result<Json<MockTopupOut>, ApiError> {
    if !state.mock_card {
        return Err(ApiError::not_implemented(
            "mock card top-ups are disabled (CREDITS_MOCK_CARD unset)",
        ));
    }
    let signer = signer_from(&headers, "post", "/topup/mock-card")?;
    let auth_ts = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let credits = crate::handlers::admin::validate_positive_amount(&body.credits)?;
    if exceeds_mock_topup_cap(&credits) {
        return Err(ApiError::bad_request(format!(
            "mock top-up is capped at {MOCK_TOPUP_MAX_CREDITS} credits per request"
        )));
    }

    let idem = format!("mock-card-topup:{}:{}", signer, auth_ts);
    let detail = serde_json::json!({
        "source": "card-mock",
        "topup": true,
        "mock": true,
    });
    let outcome = state
        .credits
        .admin_grant_credits(
            &signer,
            &credits,
            "purchase",
            Some("mock card top-up (no real charge)"),
            Some("card-mock"),
            Some(&idem),
            &detail,
        )
        .await?;

    Ok(Json(MockTopupOut {
        credits_granted: credits,
        available: outcome.available,
        mock: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_pack() {
        let out = PackOut {
            sku: "starter".into(),
            title: "Starter Pack".into(),
            credits: "1000".into(),
            price_cents: 500,
            currency: "usd".into(),
            sort_order: 1,
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "sku": "starter",
                "title": "Starter Pack",
                "credits": "1000",
                "priceCents": 500,
                "currency": "usd",
                "sortOrder": 1,
            })
        );
    }

    #[test]
    fn wire_identity_pack_intent() {
        let out = PackIntentOut {
            client_secret: "pi_3ABC_secret_XYZ".into(),
            payment_intent_id: "pi_3ABC".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "clientSecret": "pi_3ABC_secret_XYZ",
                "paymentIntentId": "pi_3ABC",
            })
        );
    }

    #[test]
    fn wire_identity_mock_topup() {
        let out = MockTopupOut {
            credits_granted: "25".into(),
            available: "125".into(),
            mock: true,
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "creditsGranted": "25",
                "available": "125",
                "mock": true,
            })
        );
    }

    #[test]
    fn mock_topup_cap_allows_up_to_the_cap() {
        assert!(!exceeds_mock_topup_cap("1"));
        assert!(!exceeds_mock_topup_cap("0.5"));
        assert!(!exceeds_mock_topup_cap("9999.99"));
        assert!(!exceeds_mock_topup_cap("10000"));
        assert!(!exceeds_mock_topup_cap("10000.00"));
        assert!(!exceeds_mock_topup_cap("010000"));
    }

    #[test]
    fn mock_topup_cap_rejects_above_the_cap() {
        assert!(exceeds_mock_topup_cap("10000.01"));
        assert!(exceeds_mock_topup_cap("10001"));
        assert!(exceeds_mock_topup_cap("99999"));
        assert!(exceeds_mock_topup_cap("100000"));
        assert!(exceeds_mock_topup_cap(&format!("1{}", "0".repeat(70))));
    }

    #[test]
    fn validates_sku() {
        assert_eq!(validate_sku(" pack_100 ").unwrap(), "pack_100");
        assert!(validate_sku("").is_err());
        assert!(validate_sku("   ").is_err());
        assert!(validate_sku("a/b").is_err());
        assert!(validate_sku("bad sku").is_err());
        assert!(validate_sku(&"x".repeat(101)).is_err());
    }

    fn pack() -> crate::ports::packs::PackRow {
        crate::ports::packs::PackRow {
            sku: "pack_100".into(),
            title: "100 Credits".into(),
            credits: "100".into(),
            price_cents: 999,
            currency: "usd".into(),
            sort_order: 0,
        }
    }

    #[test]
    fn idempotency_key_is_deterministic_and_well_formed() {
        let k1 = intent_idempotency_key("0xabc", &pack(), "1690000000000");
        let k2 = intent_idempotency_key("0xabc", &pack(), "1690000000000");
        assert_eq!(k1, k2);
        assert!(k1.starts_with("landiler-intent-"));
        assert_eq!(k1.len(), "landiler-intent-".len() + 64);
    }

    #[test]
    fn idempotency_key_varies_per_distinct_purchase() {
        let base = intent_idempotency_key("0xabc", &pack(), "1690000000000");

        assert_ne!(
            base,
            intent_idempotency_key("0xdef", &pack(), "1690000000000")
        );
        assert_ne!(
            base,
            intent_idempotency_key("0xabc", &pack(), "1690000099999")
        );
        let mut p = pack();
        p.sku = "pack_500".into();
        assert_ne!(base, intent_idempotency_key("0xabc", &p, "1690000000000"));
        let mut p = pack();
        p.price_cents = 1099;
        assert_ne!(base, intent_idempotency_key("0xabc", &p, "1690000000000"));
        let mut p = pack();
        p.credits = "120".into();
        assert_ne!(base, intent_idempotency_key("0xabc", &p, "1690000000000"));
    }
}
