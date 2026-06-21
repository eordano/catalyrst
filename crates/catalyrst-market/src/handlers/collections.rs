use axum::extract::{Query, State};
use axum::Json;

use crate::dcl_schemas::Network;
use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::collections::{Collection, CollectionFilters, CollectionSortBy};
use crate::AppState;

pub async fn get_collections(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Collection>>, ApiError> {
    let filters = parse_filters(&pairs);
    let (data, total) = state.collections.get_collections(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

fn parse_filters(pairs: &[(String, String)]) -> CollectionFilters {
    let p = Params::new(pairs);

    let first = p.get_number("first", None).map(|f| f as i64);
    let skip = p.get_number("skip", None).map(|f| f as i64);

    let sort_by = p
        .get_value(
            "sortBy",
            &[
                "newest",
                "recently_reviewed",
                "name",
                "size",
                "recently_listed",
            ],
            None,
        )
        .map(|s| match s.as_str() {
            "newest" => CollectionSortBy::Newest,
            "recently_reviewed" => CollectionSortBy::RecentlyReviewed,
            "name" => CollectionSortBy::Name,
            "size" => CollectionSortBy::Size,
            "recently_listed" => CollectionSortBy::RecentlyListed,
            _ => unreachable!(),
        });

    let name = p.get_string("name", None);
    let search = p.get_string("search", None);
    let creator = p.get_address("creator", false, None);
    let urn = p.get_string("urn", None);
    let contract_address = p.get_address("contractAddress", false, None);
    let is_on_sale = p.get_boolean("isOnSale");
    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            "MATIC" => Network::Matic,
            _ => unreachable!(),
        });

    CollectionFilters {
        first,
        skip,
        sort_by,
        name,
        search,
        creator,
        urn,
        contract_address,
        is_on_sale,
        network,
    }
}
