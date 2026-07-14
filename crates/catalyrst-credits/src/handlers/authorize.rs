use alloy_primitives::keccak256;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::handlers::{random_32_hex, signer_from};
use crate::http::ApiError;
use crate::ports::authorize::NewAuthorization;
use crate::AppState;

const AUTHORIZATION_TTL_MS: i64 = 10 * 60 * 1000;
const MAX_RELEASE_SALTS: usize = 100;

#[derive(Debug, Deserialize)]
pub struct AuthorizeBody {
    #[serde(rename = "usdPriceCents")]
    usd_price_cents: i64,
    #[serde(rename = "tradeId", default)]
    trade_id: Option<String>,
    #[serde(rename = "contractAddress", default)]
    contract_address: Option<String>,
    #[serde(rename = "itemId", default)]
    item_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct AuthorizedCreditOut {
    pub id: String,
    pub amount: String,
    #[serde(rename = "availableAmount")]
    pub available_amount: String,
    #[serde(rename = "expiresAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
    pub signature: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct AuthorizeCreditOut {
    pub credit: AuthorizedCreditOut,
    #[serde(rename = "maxCreditedValue")]
    pub max_credited_value: String,
    #[serde(rename = "usdCents")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub usd_cents: i64,
    #[serde(rename = "oracleRate")]
    pub oracle_rate: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseIntentsBody {
    salts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct ReleaseIntentsOut {
    pub ok: bool,
}

fn present(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| !s.trim().is_empty())
}

pub(crate) fn authorize_digest(
    id: &str,
    contract: &str,
    signer_address: &str,
    amount_wei: &str,
    expires_at_ms: i64,
) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(id.as_bytes());
    buf.extend_from_slice(contract.as_bytes());
    buf.extend_from_slice(signer_address.as_bytes());
    buf.extend_from_slice(amount_wei.as_bytes());
    buf.extend_from_slice(expires_at_ms.to_string().as_bytes());
    keccak256(&buf).0
}

pub async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<AuthorizeCreditOut>, ApiError> {
    let signer = signer_from(&headers, "post", "/credits/authorize").await?;
    let body: AuthorizeBody =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("invalid request body"))?;

    if body.usd_price_cents < 1 {
        return Err(ApiError::bad_request("usdPriceCents must be at least 1"));
    }
    let has_trade = present(&body.trade_id);
    let has_mint = present(&body.contract_address) && present(&body.item_id);
    if !has_trade && !has_mint {
        return Err(ApiError::bad_request(
            "authorize requires tradeId or both contractAddress and itemId",
        ));
    }

    let (Some(signer_key), Some(contract)) = (
        state.credits_signer_key.as_ref(),
        state.credits_manager_contract.as_ref(),
    ) else {
        return Err(ApiError::service_unavailable(
            "credit authorization is unavailable (signer key or CreditsManager contract unset)",
        ));
    };

    let available = state
        .credits
        .user_credits(signer.as_str())
        .await?
        .map(|c| c.available)
        .unwrap_or(0.0);
    let spendable_cents = (available * 100.0).round() as i64;
    if body.usd_price_cents > spendable_cents {
        return Err(ApiError::payment_required("insufficient credit balance"));
    }

    let mana_usd = state.pricing.fetch_mana_usd().await?;
    let (amount_wei, oracle_rate) = state
        .credits
        .usd_cents_to_mana_wei(body.usd_price_cents, &mana_usd)
        .await?;

    let id = format!("0x{}", random_32_hex());
    let expires_at_ms = chrono::Utc::now().timestamp_millis() + AUTHORIZATION_TTL_MS;

    let wallet = catalyrst_crypto::sign::Wallet::from_hex(signer_key)
        .map_err(|e| ApiError::Internal(format!("invalid credits signer key: {e}")))?;
    let digest = authorize_digest(&id, contract, signer.as_str(), &amount_wei, expires_at_ms);
    let signature = wallet
        .sign_message(&digest)
        .map_err(|e| ApiError::Internal(format!("credit signing failed: {e}")))?;

    let expires_at = chrono::DateTime::from_timestamp_millis(expires_at_ms)
        .ok_or_else(|| ApiError::Internal("expiry timestamp out of range".into()))?;

    state
        .credits
        .insert_authorization(&NewAuthorization {
            id: &id,
            address: signer.as_str(),
            usd_cents: body.usd_price_cents,
            amount_wei: &amount_wei,
            trade_id: body.trade_id.as_deref(),
            contract_address: body.contract_address.as_deref(),
            item_id: body.item_id.as_deref(),
            source: body.source.as_deref(),
            expires_at,
        })
        .await?;

    Ok(Json(AuthorizeCreditOut {
        credit: AuthorizedCreditOut {
            id,
            amount: amount_wei.clone(),
            available_amount: amount_wei.clone(),
            expires_at: expires_at_ms,
            signature,
            contract: contract.clone(),
        },
        max_credited_value: amount_wei,
        usd_cents: body.usd_price_cents,
        oracle_rate,
    }))
}

pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<ReleaseIntentsOut>, ApiError> {
    let signer = signer_from(&headers, "post", "/credits/authorize/cancel").await?;
    let body: ReleaseIntentsBody =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("invalid request body"))?;

    if body.salts.is_empty() || body.salts.len() > MAX_RELEASE_SALTS {
        return Err(ApiError::bad_request(
            "salts must contain between 1 and 100 entries",
        ));
    }

    state
        .credits
        .release_intents(&body.salts, signer.as_str())
        .await?;

    Ok(Json(ReleaseIntentsOut { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_authorize_credit() {
        let out = AuthorizeCreditOut {
            credit: AuthorizedCreditOut {
                id: "0xabc".into(),
                amount: "1000000000000000000".into(),
                available_amount: "1000000000000000000".into(),
                expires_at: 1_690_000_600_000,
                signature: "0xsig".into(),
                contract: "0xcontract".into(),
            },
            max_credited_value: "1000000000000000000".into(),
            usd_cents: 500,
            oracle_rate: "420000000000000000".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "credit": {
                    "id": "0xabc",
                    "amount": "1000000000000000000",
                    "availableAmount": "1000000000000000000",
                    "expiresAt": 1_690_000_600_000i64,
                    "signature": "0xsig",
                    "contract": "0xcontract",
                },
                "maxCreditedValue": "1000000000000000000",
                "usdCents": 500,
                "oracleRate": "420000000000000000",
            })
        );
    }

    #[test]
    fn wire_identity_release() {
        assert_eq!(
            serde_json::to_value(ReleaseIntentsOut { ok: true }).unwrap(),
            json!({ "ok": true })
        );
    }

    #[test]
    fn digest_layout_is_pinned() {
        let d = authorize_digest("0x00", "0xcontract", "0xsigner", "1000", 1_690_000_600_000);
        let mut expected = Vec::new();
        expected.extend_from_slice(b"0x00");
        expected.extend_from_slice(b"0xcontract");
        expected.extend_from_slice(b"0xsigner");
        expected.extend_from_slice(b"1000");
        expected.extend_from_slice(b"1690000600000");
        assert_eq!(d, keccak256(&expected).0);
    }
}
