use axum::extract::{Query, State};
use axum::Json;
use serde_json::json;

use crate::http::pagination::get_pagination_params;
use crate::http::response::ApiError;
use crate::ports::bids::parse_filters;
use crate::AppState;

pub async fn get_bids(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pg = get_pagination_params(&pairs);
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.bids.get_bids(&filters).await?;
    let page = if pg.limit > 0 { pg.offset / pg.limit } else { 0 };
    let pages = if !data.is_empty() && pg.limit > 0 {
        (total + pg.limit - 1) / pg.limit
    } else {
        0
    };
    Ok(Json(json!({
        "ok": true,
        "data": {
            "results": data,
            "total": total,
            "page": page,
            "pages": pages,
            "limit": pg.limit,
        }
    })))
}
