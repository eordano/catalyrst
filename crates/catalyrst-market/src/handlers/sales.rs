use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::{ApiError, DataTotal};
use crate::ports::sales::{parse_filters, Sale};
use crate::AppState;

pub async fn get_sales(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Sale>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.sales.get_sales(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}
