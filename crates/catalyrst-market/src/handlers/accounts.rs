use axum::extract::{Query, State};
use axum::Json;

use crate::dcl_schemas::Network;
use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::accounts::{Account, AccountFilters, AccountSortBy};
use crate::AppState;

pub async fn get_accounts(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Account>>, ApiError> {
    let filters = parse_filters(&pairs);
    let (data, total) = state.accounts.get_accounts(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}

fn parse_filters(pairs: &[(String, String)]) -> AccountFilters {
    let p = Params::new(pairs);

    let first = p.get_number("first", None).map(|f| f as i64);
    let skip = p.get_number("skip", None).map(|f| f as i64);

    let sort_by = p
        .get_value(
            "sortBy",
            &[
                "most_sales",
                "most_purchases",
                "most_royalties",
                "most_collections",
                "most_earned",
                "most_spent",
            ],
            None,
        )
        .map(|s| match s.as_str() {
            "most_sales" => AccountSortBy::MostSales,
            "most_purchases" => AccountSortBy::MostPurchases,
            "most_royalties" => AccountSortBy::MostRoyalties,
            "most_collections" => AccountSortBy::MostCollections,
            "most_earned" => AccountSortBy::MostEarned,
            "most_spent" => AccountSortBy::MostSpent,
            _ => unreachable!(),
        });

    let id = p.get_string("id", None);
    let address = p.get_list("address", &[]);
    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            "MATIC" => Network::Matic,
            _ => unreachable!(),
        });

    AccountFilters {
        first,
        skip,
        sort_by,
        id,
        address,
        network,
    }
}
