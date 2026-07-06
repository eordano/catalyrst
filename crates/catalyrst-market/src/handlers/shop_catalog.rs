use axum::extract::{Query, State};
use axum::Json;
use serde::Serialize;

use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::shop_catalog::{
    parse_legacy_filters, parse_shop_filters, ImportableListing, LegacyListing, ShopListing,
};
use crate::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct ImportableResponseBody {
    pub data: Vec<ImportableListing>,
}

// GET /v3/catalog/shop — curated feed of credit-buyable (USD-pegged) listings for the Shop.
pub async fn get_shop_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<ShopListing>>, ApiError> {
    let filters = parse_shop_filters(&pairs);
    let (data, total) = state.shop_catalog.get_shop_listings(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

// GET /v3/catalog/legacy — paginated feed of classic MANA-priced PRIMARY listings (the "old
// liquidity") so the Shop can offer them for purchase with credits. Returns the raw MANA price
// (manaWei); the client converts to credits via the oracle.
pub async fn get_legacy_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<LegacyListing>>, ApiError> {
    let filters = parse_legacy_filters(&pairs);
    let (data, total) = state.shop_catalog.get_legacy_listings(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

// GET /v3/catalog/importable?seller=0x... — a seller's OLD classic (MANA-priced) listings they
// can import into the Shop. Public read (open orders are already public).
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
