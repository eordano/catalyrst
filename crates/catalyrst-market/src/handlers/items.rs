use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::ApiError;
use crate::ports::items::{parse_filters, Item};
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ItemsResponseBody {
    pub data: Vec<Item>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

pub async fn get_items(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ItemsResponseBody>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let (data, total) = state.items.get_items(&filters).await?;
    Ok(Json(ItemsResponseBody { data, total }))
}
