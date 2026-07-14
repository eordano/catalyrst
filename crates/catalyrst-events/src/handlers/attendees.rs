use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth_chain::require_signer;
use crate::http::response::{ApiError, ApiOk};
use crate::schemas::EventAttendeeRecord;
use crate::AppState;

#[utoipa::path(
    get,
    path = "/api/events/{event_id}/attendees",
    tag = "events",
    params(("event_id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<Vec<EventAttendeeRecord>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_event_attendees(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<ApiOk<Vec<EventAttendeeRecord>>>, ApiError> {
    let list = state.attendees.list_for_event(&event_id).await?;
    Ok(Json(ApiOk::new(list)))
}

async fn require_auth(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, ApiError> {
    require_signer(headers, method, path).await
}

#[utoipa::path(
    post,
    path = "/api/events/{event_id}/attendees",
    tag = "events",
    params(("event_id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<Vec<EventAttendeeRecord>>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn create_event_attendee(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiOk<Vec<EventAttendeeRecord>>>, ApiError> {
    let path = format!("/api/events/{}/attendees", event_id);
    let signer = require_auth(&headers, "post", &path).await?;

    if !state
        .events
        .exists_visible(&event_id, signer.as_str())
        .await?
    {
        return Err(ApiError::not_found(format!(
            "Not found event \"{}\"",
            event_id
        )));
    }

    let user_name = headers
        .get(crate::auth_chain::AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|m| m.get("name").and_then(|n| n.as_str()).map(String::from));

    let list = state
        .attendees
        .rsvp_going(
            &event_id,
            signer.as_str(),
            user_name.as_deref(),
            serde_json::Value::Null,
        )
        .await?;
    Ok(Json(ApiOk::new(list)))
}

#[utoipa::path(
    delete,
    path = "/api/events/{event_id}/attendees",
    tag = "events",
    params(("event_id" = String, Path)),
    responses(
        (status = 200, body = ApiOk<Vec<EventAttendeeRecord>>),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_event_attendee(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiOk<Vec<EventAttendeeRecord>>>, ApiError> {
    let path = format!("/api/events/{}/attendees", event_id);
    let signer = require_auth(&headers, "delete", &path).await?;

    if !state
        .events
        .exists_visible(&event_id, signer.as_str())
        .await?
    {
        return Err(ApiError::not_found(format!(
            "Not found event \"{}\"",
            event_id
        )));
    }

    let list = state
        .attendees
        .rsvp_cancel(&event_id, signer.as_str())
        .await?;
    Ok(Json(ApiOk::new(list)))
}
