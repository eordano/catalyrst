use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::admin::authorize_admin;
use crate::http::ApiError;
use crate::ports::NOTIFICATION_TYPES;
use crate::AppState;

const MAX_ADDRESSES: usize = 10_000;

#[derive(Debug, Deserialize)]
pub struct BroadcastBody {
    #[serde(rename = "type")]
    pub kind: String,

    #[serde(default)]
    pub metadata: JsonValue,

    #[serde(default)]
    pub addresses: Option<Vec<String>>,
}

pub async fn post_broadcast(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BroadcastBody>,
) -> Result<impl IntoResponse, ApiError> {
    authorize_admin(&state, &headers)?;

    if !NOTIFICATION_TYPES.contains(&body.kind.as_str()) {
        return Err(ApiError::bad_request(format!(
            "unknown notification type: {}",
            body.kind
        )));
    }

    if !body.metadata.is_null() && !body.metadata.is_object() {
        return Err(ApiError::bad_request("metadata must be an object"));
    }
    let metadata = if body.metadata.is_null() {
        serde_json::json!({})
    } else {
        body.metadata
    };

    let addresses: Option<Vec<String>> = match body.addresses {
        Some(list) => {
            if list.is_empty() {
                return Err(ApiError::bad_request(
                    "addresses must be non-empty when provided (omit it to broadcast to all)",
                ));
            }
            if list.len() > MAX_ADDRESSES {
                return Err(ApiError::bad_request(format!(
                    "too many addresses: {} (max {})",
                    list.len(),
                    MAX_ADDRESSES
                )));
            }
            for a in &list {
                if !catalyrst_types::is_eth_address(a) {
                    return Err(ApiError::bad_request(format!("invalid address: {}", a)));
                }
            }
            Some(list)
        }
        None => None,
    };

    let broadcast_id = Uuid::new_v4().to_string();
    let inserted = state
        .notifications
        .broadcast(&broadcast_id, &body.kind, &metadata, addresses.as_deref())
        .await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "broadcastId": broadcast_id,
            "type": body.kind,
            "recipients": inserted,
        })),
    ))
}
