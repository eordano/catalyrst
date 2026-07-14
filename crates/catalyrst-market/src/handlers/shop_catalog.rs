use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::shop_catalog::{
    parse_legacy_filters, parse_shop_filters, parse_unified_filters, parse_unified_group_by,
    ImportableListing, LegacyListing, ShopListing, UnifiedGroupBy, UnifiedItem,
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
) -> Result<Response, ApiError> {
    let filters = parse_unified_filters(&pairs);
    let rate = state.mana_usd_rate.get_rate();
    match parse_unified_group_by(&pairs) {
        UnifiedGroupBy::Item => {
            let (data, total) = state.shop_catalog.get_shop_items(&filters, rate).await?;
            Ok(Json(DataTotal { data, total }).into_response())
        }
        UnifiedGroupBy::Listing => {
            let (data, total) = state
                .shop_catalog
                .get_unified_listings(&filters, rate)
                .await?;
            Ok(Json(DataTotal { data, total }).into_response())
        }
    }
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

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "market/"))]
pub struct RelatedResponseBody {
    pub data: Vec<UnifiedItem>,
}

/// Blockchain ids are unbounded non-negative integers, so the whole constraint
/// is a digit check (NOT a u64 parse): a huge id must still be accepted.
fn is_numeric_item_id(item_id: &str) -> bool {
    !item_id.is_empty() && item_id.bytes().all(|b| b.is_ascii_digit())
}

/// GET /v3/catalog/related — items SIMILAR to one item, backing the PDP's
/// fallback rail. Same item-unified, credit-priced shape as
/// /v3/catalog/unified?groupBy=item. Unpaginated: `{ data }` only. A malformed
/// deep link (absent/invalid contractAddress, or an itemId that is not a plain
/// digit string) yields an empty rail WITHOUT touching the DB, so a bad URL
/// never reaches `blockchain_id = itemId::numeric` and 500s.
pub async fn get_related_catalog(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<RelatedResponseBody>, ApiError> {
    let params = Params::new(&pairs);
    let contract_address = params.get_address("contractAddress", true, None);
    let item_id = params.get_string("itemId", None);
    let first = params
        .get_number("first", None)
        .filter(|n| n.is_finite())
        .map(|n| n as i64);

    let data = match (contract_address, item_id) {
        (Some(contract_address), Some(item_id)) if is_numeric_item_id(&item_id) => {
            let rate = state.mana_usd_rate.get_rate();
            state
                .shop_catalog
                .get_related_items(&contract_address, &item_id, first, rate)
                .await?
        }
        _ => Vec::new(),
    };
    Ok(Json(RelatedResponseBody { data }))
}

#[cfg(test)]
mod tests {
    use super::is_numeric_item_id;

    #[test]
    fn numeric_item_id_guard_keeps_non_digit_ids_away_from_the_numeric_cast() {
        // A non-numeric id used to reach `blockchain_id = itemId::numeric` and 500 a public GET
        // from a malformed /item/:contractAddress/:itemId deep link. All of these must be rejected.
        for junk in [
            "abc",
            "1e3",
            "-1",
            "1.5",
            "3; DROP TABLE item",
            " ",
            "0x03",
            "",
        ] {
            assert!(!is_numeric_item_id(junk), "{junk:?} must be rejected");
        }

        assert!(is_numeric_item_id("3"));
        // Blockchain ids are unbounded integers, well past 2^53 — a digit check must still accept them.
        assert!(is_numeric_item_id("90071992547409910000"));
    }
}
