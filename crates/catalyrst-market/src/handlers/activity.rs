use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::pagination::get_number_parameter;
use crate::http::response::ApiError;
use crate::ports::activity::{ActivityEvent, ActivityOptions};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ActivityEnvelope {
    pub data: Vec<ActivityEvent>,
    pub total: i64,
}

pub async fn get_activity(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ActivityEnvelope>, ApiError> {
    let address = pairs
        .iter()
        .find(|(k, _)| k == "address")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| ApiError::bad_request("Unauthorized"))?;

    let limit = get_number_parameter("limit", &pairs)?;
    let offset = get_number_parameter("offset", &pairs)?;

    let (data, total) = state
        .activity
        .get_user_activity(&address, ActivityOptions { limit, offset })
        .await?;

    Ok(Json(ActivityEnvelope { data, total }))
}
