//! Admin broadcast endpoint.
//!
//! `POST /notifications/broadcast` fans out a single admin notification to every
//! known subscriber, or to a caller-supplied set of addresses. Gated by a bearer
//! token (`CATALYRST_NOTIFICATIONS_ADMIN_TOKEN`) which fails closed when unset.
//! This is additive: it does not alter the existing SignedFetch reader routes.

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

/// Max number of explicit target addresses accepted in one broadcast request.
const MAX_ADDRESSES: usize = 10_000;

#[derive(Debug, Deserialize)]
pub struct BroadcastBody {
    /// Notification type; must be one of the known `NOTIFICATION_TYPES`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Arbitrary metadata stored verbatim on each notification row.
    #[serde(default)]
    pub metadata: JsonValue,
    /// Optional explicit recipient set. When omitted the broadcast targets every
    /// address present in the `subscriptions` table.
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

    // Validate the explicit address set up-front (empty list is rejected so a
    // caller never silently broadcasts to nobody when they meant to target some).
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
