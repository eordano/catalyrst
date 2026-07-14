use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::Utc;
use serde::Serialize;

use catalyrst_crypto::signed_fetch::signed_fetch_path;

use crate::auth_chain::{
    self, build_payload, AuthChainError, AuthChainErrorExt, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
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

#[utoipa::path(
    get,
    path = "/v1/activity",
    tag = "market",
    params(("address" = String, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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

    auth_chain::require_auth_metadata(&headers, auth_chain::MARKETPLACE_AUTH_SIGNERS, None)?;

    let path = signed_fetch_path(&headers, "/v1/activity");
    let payload = build_payload("get", path.as_ref(), timestamp, metadata);

    let now = Utc::now().timestamp();
    let recovered = auth_chain::validate_signature(&chain, &payload, timestamp, FIVE_MINUTES, now)
        .await
        .map_err(auth_chain_error_to_api)?;

    let query_address = pairs
        .iter()
        .find(|(k, _)| k == "address")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| ApiError::bad_request("Unauthorized"))?;

    if recovered.as_str() != query_address.to_lowercase() {
        return Err(auth_chain_error_to_api(AuthChainError::AddressMismatch {
            expected: query_address.to_lowercase(),
            recovered: recovered.as_str().to_string(),
        }));
    }

    let limit = get_number_parameter("limit", &pairs)?;
    let offset = get_number_parameter("offset", &pairs)?;

    let (data, total) = state
        .activity
        .get_user_activity(recovered.as_str(), ActivityOptions { limit, offset })
        .await?;

    Ok(Json(ActivityEnvelope { data, total }))
}
