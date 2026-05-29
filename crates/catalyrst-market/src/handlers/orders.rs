//! Direct port of `marketplace-server/src/controllers/handlers/orders-handler.ts`.

use axum::extract::{Query, State};
use axum::Json;

use crate::http::response::{ApiError, DataTotal};
use crate::ports::orders::{parse_filters, Order};
use crate::AppState;

pub async fn get_orders(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Order>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.orders.get_orders(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}
