use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::{ApiError, DataOnly, DataTotal};
use crate::ports::sales::{parse_filters, parse_summary_filters, Sale, SalesSummary};
use crate::AppState;

pub async fn get_sales(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Sale>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.sales.get_sales(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

pub async fn get_sales_summary(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataOnly<SalesSummary>>, ApiError> {
    let filters = parse_summary_filters(&pairs)?;
    let data = state.sales.get_summary(&filters).await?;
    Ok(Json(DataOnly { data }))
}
