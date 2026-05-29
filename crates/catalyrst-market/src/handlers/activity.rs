use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use serde::Serialize;

use crate::auth_chain::{
    self, build_payload, AuthChainError, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
};
use crate::http::pagination::get_number_parameter;
use crate::http::response::ApiError;
use crate::ports::activity::{ActivityEvent, ActivityOptions};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ActivityEnvelope {
    pub data: Vec<ActivityEvent>,
    pub total: i64,
}

/// Convert an `AuthChainError` into the prod-shape `{ok:false, message:...}`
/// envelope. Status codes mirror `@dcl/crypto-middleware`:
///   - shape/structural failures → 400
///   - expired or bad-signature failures → 401
///   - address mismatch → 400 (spec — distinct from prod which uses 401)
fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::AddressMismatch { .. } => ApiError::bad_request(e.message()),
        AuthChainError::Expired { .. } | AuthChainError::InvalidSignature(_) => {
            // 401 status with the canonical envelope. ApiError doesn't have a
            // `.unauthorized()` helper today, so we synthesize via HttpError.
            ApiError::Http(catalyrst_types::HttpError::new(401, e.message()))
        }
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        // Shape failures all collapse to the canonical "Invalid Auth Chain"
        // bytes for parity with prod.
        _ => ApiError::bad_request(e.message()),
    }
}

pub async fn get_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ActivityEnvelope>, ApiError> {
    // ------------------------------------------------------------------
    // 1. Extract + structurally validate the auth chain.
    // ------------------------------------------------------------------
    let chain = auth_chain::extract_auth_chain(&headers).map_err(auth_chain_error_to_api)?;

    // ------------------------------------------------------------------
    // 2. Recover the canonical timestamp + metadata, build the signed
    //    payload, and cryptographically verify the chain.
    //
    //    Canonical payload:
    //      "get:/v1/activity:<x-identity-timestamp>:<x-identity-metadata>"
    //    Header values are taken verbatim then lowercased end-to-end
    //    (matches `createPayload` in @dcl/crypto-middleware).
    // ------------------------------------------------------------------
    let timestamp = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| auth_chain_error_to_api(AuthChainError::MissingTimestamp))?;
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("{}");

    let payload = build_payload("get", "/v1/activity", timestamp, metadata);

    let now = Utc::now().timestamp();
    let recovered = auth_chain::validate_signature(&chain, &payload, FIVE_MINUTES, now)
        .map_err(auth_chain_error_to_api)?;

    // ------------------------------------------------------------------
    // 3. Compare recovered signer to `?address=`. Do NOT trust the query
    //    string. If it doesn't match the cryptographically recovered
    //    address, this is a forgery attempt — fail loudly.
    // ------------------------------------------------------------------
    let query_address = pairs
        .iter()
        .find(|(k, _)| k == "address")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| ApiError::bad_request("Unauthorized"))?;

    if recovered.to_lowercase() != query_address.to_lowercase() {
        return Err(auth_chain_error_to_api(AuthChainError::AddressMismatch {
            expected: query_address.to_lowercase(),
            recovered,
        }));
    }

    // ------------------------------------------------------------------
    // 4. Authenticated — proceed with the actual activity query.
    //    Use the recovered (verified) address, not the query string, to
    //    seal off any case-difference / encoding-difference exploits.
    // ------------------------------------------------------------------
    let limit = get_number_parameter("limit", &pairs)?;
    let offset = get_number_parameter("offset", &pairs)?;

    let (data, total) = state
        .activity
        .get_user_activity(&recovered, ActivityOptions { limit, offset })
        .await?;

    Ok(Json(ActivityEnvelope { data, total }))
}
