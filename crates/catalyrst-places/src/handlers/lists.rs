use axum::extract::State;
use axum::Json;

use crate::http::errors::ApiError;
use crate::http::response::ListResponse;
use crate::AppState;

pub async fn post_pois(State(state): State<AppState>) -> Result<Json<ListResponse>, ApiError> {
    let coords = state.lists.pois().await?;
    Ok(Json(ListResponse::new(coords)))
}

pub async fn post_banned_names(
    State(state): State<AppState>,
) -> Result<Json<ListResponse>, ApiError> {
    let names = state.lists.banned_names().await?;
    Ok(Json(ListResponse::new(names)))
}
