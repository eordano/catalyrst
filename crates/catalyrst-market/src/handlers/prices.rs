use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::ApiError;
use crate::ports::prices::{parse_filters, PricesEnvelope};
use crate::AppState;

pub async fn get_prices(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<PricesEnvelope>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let data = state.prices.get_prices(&filters).await?;
    Ok(Json(PricesEnvelope { data }))
}
