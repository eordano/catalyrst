use std::sync::Arc;

use axum::extract::{Request, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppError, AppResult};
use crate::query_params::{parse_query_string, qs_get_number};
use crate::state::AppState;

// Largest page returned when the caller opts into pagination via `?offset=&limit=`.
const MAX_FAILED_DEPLOYMENTS_PAGE_SIZE: i64 = 1000;

// Method: GET
// Query String (optional): ?offset={n}&limit={n}
pub async fn get_failed_deployments(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let params = parse_query_string(request.uri().query().unwrap_or(""));
    let offset = qs_get_number(&params, "offset");
    let limit = qs_get_number(&params, "limit");

    let failed = state
        .database
        .get_failed_deployments()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Backward-compatible, opt-in pagination: with neither `offset` nor `limit` we return the full
    // array exactly as before (the response is a bare array, so a pagination envelope would break the
    // API contract). When either is provided we return a bounded slice, so an operator can page a
    // large failure set instead of pulling the entire in-memory table — which includes every auth
    // chain and internal error description — in one response.
    if offset.is_none() && limit.is_none() {
        return Ok(Json(failed));
    }

    let safe_offset = offset.filter(|&o| o > 0).unwrap_or(0) as usize;
    let safe_limit = match limit {
        Some(l) => l.clamp(0, MAX_FAILED_DEPLOYMENTS_PAGE_SIZE) as usize,
        None => MAX_FAILED_DEPLOYMENTS_PAGE_SIZE as usize,
    };

    let page: Vec<_> = failed
        .into_iter()
        .skip(safe_offset)
        .take(safe_limit)
        .collect();

    Ok(Json(page))
}
