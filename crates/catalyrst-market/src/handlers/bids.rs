use axum::extract::{Query, State};
use axum::Json;

use crate::http::pagination::get_pagination_params;
use crate::http::response::{ApiError, PaginatedResponse};
use crate::ports::bids::{parse_filters, Bid};
use crate::AppState;

pub async fn get_bids(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<PaginatedResponse<Bid>>, ApiError> {
    let pg = get_pagination_params(&pairs);
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.bids.get_bids(&filters).await?;
    Ok(Json(PaginatedResponse::new(
        data, total, pg.limit, pg.offset,
    )))
}
