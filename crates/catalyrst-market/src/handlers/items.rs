use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::response::{ApiError, DataTotal};
use crate::ports::items::{parse_filters, CreditCatalogItem, Item};
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

// GET /v3/catalog/items — the credit-aware CATALOG-ITEMS feed. Same data source, params and
// full-catalog semantics as GET /v1/items (ALL items incl. not-on-sale, keyed by item) but every
// item carries a server-computed, asset-type-aware priceCredits (USD-pegged trades pass through;
// MANA-priced ones are converted with the live MANA/USD rate).
pub async fn get_catalog_items(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<CreditCatalogItem>>, ApiError> {
    let filters = parse_filters(&pairs)?;
    let rate = state.mana_usd_rate.get_rate();
    let (data, total) = state.items.get_catalog_items(&filters, rate).await?;
    Ok(Json(DataTotal { data, total }))
}
