use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::{ApiError, DataTotalString};
use crate::ports::orders::{parse_filters, Order};
use crate::AppState;

pub async fn get_orders(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotalString<Order>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.orders.get_orders(&filters).await?;
    Ok(Json(DataTotalString {
        data,
        total: total.to_string(),
    }))
}
