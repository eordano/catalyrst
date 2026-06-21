use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::shop_catalog::{
    parse_legacy_filters, parse_shop_filters, parse_unified_filters, ImportableListing,
    LegacyListing, ShopListing, UnifiedListing,
};
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ImportableResponseBody {
    pub data: Vec<ImportableListing>,
}

pub async fn get_shop_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<ShopListing>>, ApiError> {
    let filters = parse_shop_filters(&pairs);
    let (data, total) = state.shop_catalog.get_shop_listings(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

pub async fn get_legacy_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<LegacyListing>>, ApiError> {
    let filters = parse_legacy_filters(&pairs);
    let (data, total) = state.shop_catalog.get_legacy_listings(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

pub async fn get_unified_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<UnifiedListing>>, ApiError> {
    let filters = parse_unified_filters(&pairs);
    let rate = state.mana_usd_rate.get_rate();
    let (data, total) = state
        .shop_catalog
        .get_unified_listings(&filters, rate)
        .await?;
    Ok(Json(DataTotal { data, total }))
}

pub async fn get_importable_listings(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ImportableResponseBody>, ApiError> {
    let seller = Params::new(&pairs).get_address("seller", true, None);
    let data = match seller {
        Some(seller) => state.shop_catalog.get_importable_listings(&seller).await?,
        None => Vec::new(),
    };
    Ok(Json(ImportableResponseBody { data }))
}
