use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::http::ApiError;
use crate::ports::{MarkReadResponse, NotificationsListResponse};
use crate::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub from: Option<i64>,
    #[serde(rename = "onlyUnread")]
    pub only_unread: Option<bool>,
}

pub async fn get_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<NotificationsListResponse>, ApiError> {
    let signer = require_signer(&headers, "get", "/notifications").await?;

    let limit = match q.limit {
        Some(n) if n > 0 && n <= MAX_LIMIT => n,
        Some(n) if n > MAX_LIMIT => MAX_LIMIT,
        _ => DEFAULT_LIMIT,
    };
    let only_unread = q.only_unread.unwrap_or(false);

    state.notifications.touch_reader_seen(signer.as_str()).await;
    let items = state
        .notifications
        .list(signer.as_str(), limit, q.from, only_unread)
        .await?;

    Ok(Json(NotificationsListResponse {
        notifications: items,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ReadBody {
    #[serde(rename = "notificationIds")]
    pub notification_ids: Vec<String>,
}

pub async fn put_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReadBody>,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "put", "/notifications/read").await?;

    let ids: Vec<Uuid> = body
        .notification_ids
        .iter()
        .map(|s| Uuid::parse_str(s))
        .collect::<Result<_, _>>()
        .map_err(|_| ApiError::bad_request("invalid notification id"))?;

    let updated = state.notifications.mark_read(signer.as_str(), &ids).await?;
    Ok(Json(MarkReadResponse { updated }))
}
