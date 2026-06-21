use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;

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

pub async fn create_schedule() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /api/schedules is admin-only; will become a federation-signed moderator action per docs/federation/events.md §3",
    ))
}

pub async fn patch_schedule() -> Result<Json<Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PATCH /api/schedules/{id} is admin-only; will become a federation-signed moderator action per docs/federation/events.md §3",
    ))
}
