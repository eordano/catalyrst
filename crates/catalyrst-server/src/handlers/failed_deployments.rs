use std::sync::Arc;

use axum::extract::{Request, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::{AppError, AppResult};
use crate::query_params::{parse_query_string, qs_get_number};
use crate::state::AppState;

const MAX_FAILED_DEPLOYMENTS_PAGE_SIZE: i64 = 1000;

pub async fn get_failed_deployments(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let params = parse_query_string(request.uri().query().unwrap_or(""));
    let offset = qs_get_number(&params, "offset");
    let limit = qs_get_number(&params, "limit");

    if offset.is_none() && limit.is_none() {
        let failed = state
            .database
            .get_failed_deployments()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok(Json(failed));
    }

    let safe_offset = offset.filter(|&o| o > 0).unwrap_or(0);
    let safe_limit = match limit {
        Some(l) => l.clamp(0, MAX_FAILED_DEPLOYMENTS_PAGE_SIZE),
        None => MAX_FAILED_DEPLOYMENTS_PAGE_SIZE,
    };

    let page = state
        .database
        .get_failed_deployments_page(safe_offset, safe_limit)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(page))
}
