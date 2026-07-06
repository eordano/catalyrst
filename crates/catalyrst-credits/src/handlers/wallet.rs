use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::handlers::signer_from;
use crate::http::ApiError;
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct BalanceOut {
    address: String,
    available: String,
}

pub async fn balance(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BalanceOut>, ApiError> {
    let wallet = wallet_id.to_lowercase();
    let path = format!("/wallet/{}/balance", wallet_id);
    let signer = signer_from(&headers, "get", &path)?;

    if signer != wallet {
        return Err(ApiError::forbidden("walletId does not match signer"));
    }

    let available = state.credits.balance(&wallet).await?;

    Ok(Json(BalanceOut {
        address: wallet,
        available,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_balance() {
        let out = BalanceOut {
            address: "0xabc".into(),
            available: "12.5".into(),
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({ "address": "0xabc", "available": "12.5" })
        );
    }
}
