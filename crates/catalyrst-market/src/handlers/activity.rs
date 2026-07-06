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

fn signed_fetch_path<'a>(headers: &HeaderMap, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => std::borrow::Cow::Owned(raw.split('?').next().unwrap_or(raw).to_string()),
        None => std::borrow::Cow::Borrowed(fallback),
    }
}

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::AddressMismatch { .. } => ApiError::bad_request(e.message()),
        AuthChainError::Expired { .. } | AuthChainError::InvalidSignature(_) => {
            ApiError::Http(catalyrst_types::HttpError::new(401, e.message()))
        }
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }

        _ => ApiError::bad_request(e.message()),
    }
}

pub async fn get_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ActivityEnvelope>, ApiError> {
    let chain = auth_chain::extract_auth_chain(&headers).map_err(auth_chain_error_to_api)?;

    let timestamp = headers
        .get(AUTH_TIMESTAMP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| auth_chain_error_to_api(AuthChainError::MissingTimestamp))?;
    let metadata = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("{}");

    let path = signed_fetch_path(&headers, "/v1/activity");
    let payload = build_payload("get", path.as_ref(), timestamp, metadata);

    let now = Utc::now().timestamp();
    let recovered = auth_chain::validate_signature(&chain, &payload, timestamp, FIVE_MINUTES, now)
        .map_err(auth_chain_error_to_api)?;

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

    let limit = get_number_parameter("limit", &pairs)?;
    let offset = get_number_parameter("offset", &pairs)?;

    let (data, total) = state
        .activity
        .get_user_activity(&recovered, ActivityOptions { limit, offset })
        .await?;

    Ok(Json(ActivityEnvelope { data, total }))
}
