use axum::extract::State;
use axum::Json;

use crate::http::response::{ApiError, ApiOk};
use crate::schemas::EventCategoryRecord;
use crate::AppState;

pub async fn get_event_category_list(
    State(state): State<AppState>,
) -> Result<Json<ApiOk<Vec<EventCategoryRecord>>>, ApiError> {
    let list = state.categories.list().await?;
    Ok(Json(ApiOk::new(list)))
}
