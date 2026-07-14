use axum::extract::State;
use axum::Json;

use crate::http::response::{ApiError, ApiOk};
use crate::schemas::EventCategoryRecord;
use crate::AppState;

#[utoipa::path(
    get,
    path = "/api/events/categories",
    tag = "events",
    responses(
        (status = 200, body = ApiOk<Vec<EventCategoryRecord>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_event_category_list(
    State(state): State<AppState>,
) -> Result<Json<ApiOk<Vec<EventCategoryRecord>>>, ApiError> {
    let list = state.categories.list().await?;
    Ok(Json(ApiOk::new(list)))
}
