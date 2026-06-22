use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::fed::apply as fed_apply;
use crate::fed::authority;
use crate::fed::messages::ScheduleUpsert;
use crate::handlers::federation::{emit_gossip, is_federation_envelope, preflight};
use crate::http::response::{ApiError, ApiOk};
use crate::schemas::ScheduleRecord;
use crate::AppState;

pub async fn get_schedule_list(
    State(state): State<AppState>,
) -> Result<Json<ApiOk<Vec<ScheduleRecord>>>, ApiError> {
    let list = state.schedules.list().await?;
    Ok(Json(ApiOk::new(list)))
}

pub async fn get_schedule_by_id(
    State(state): State<AppState>,
    Path(schedule_id): Path<String>,
) -> Result<Json<ApiOk<ScheduleRecord>>, ApiError> {
    let s =
        state.schedules.get(&schedule_id).await?.ok_or_else(|| {
            ApiError::not_found(format!("Schedule \"{}\" not found", schedule_id))
        })?;
    Ok(Json(ApiOk::new(s)))
}

/// Federation-signed moderator schedule create. A `None` `schedule_id` in the
/// envelope is a create; the signer must be a local moderator.
pub async fn create_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    apply_upsert(&state, &headers, body, None).await
}

/// Federation-signed moderator schedule update. The envelope's `schedule_id`
/// must match the path id; the signer must be a local moderator.
pub async fn patch_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(schedule_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    apply_upsert(&state, &headers, body, Some(schedule_id)).await
}

async fn apply_upsert(
    state: &AppState,
    headers: &HeaderMap,
    body: Value,
    path_id: Option<String>,
) -> Result<Json<Value>, ApiError> {
    if !is_federation_envelope(&body) {
        return Err(ApiError::bad_request("missing signed body"));
    }
    let (signed, signer) = preflight::<ScheduleUpsert>(state, headers, body).await?;
    if let Some(id) = &path_id {
        match &signed.message.schedule_id {
            Some(body_id) if body_id == id => {}
            _ => {
                return Err(ApiError::bad_request(
                    "schedule_id in body does not match path",
                ))
            }
        }
    }
    authority::require_moderator(&state.pool, &signer).await?;
    let (applied, schedule) =
        fed_apply::apply_schedule(&state.pool, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(Json(json!({ "ok": true, "data": schedule })))
}
