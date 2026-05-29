//! Direct port of `marketplace-server/src/controllers/handlers/owners-handler.ts`.

use axum::extract::{Query, State};
use axum::Json;

use crate::http::params::Params;
use crate::http::response::{ApiError, DataTotal};
use crate::ports::owners::{Owner, OwnersFilters, OwnersSortBy};
use crate::AppState;

pub async fn get_owners(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<Owner>>, ApiError> {
    let p = Params::new(&pairs);

    let contract_address = p.get_string("contractAddress", None);
    let item_id = p.get_string("itemId", None);

    // Only the single allowed value, mirrors `params.getValue<OwnersSortBy>('sortBy', OwnersSortBy) || OwnersSortBy.ISSUED_ID`.
    let sort_by = match p.get_value("sortBy", &["issuedId"], Some("issuedId")) {
        Some(_) => Some(OwnersSortBy::IssuedId),
        None => Some(OwnersSortBy::IssuedId),
    };
    let order_direction = p.get_string("orderDirection", Some("desc"));
    let first = p.get_number("first", None).map(|f| f as i64);
    let skip = p.get_number("skip", None).map(|f| f as i64);

    let (contract_address, item_id) = match (contract_address, item_id) {
        (Some(c), Some(i)) => (c, i),
        _ => {
            return Err(ApiError::bad_request(
                "itemId and contractAddress are necessary params.",
            ))
        }
    };

    let filters = OwnersFilters {
        contract_address,
        item_id,
        first,
        skip,
        sort_by,
        order_direction,
    };

    let (data, total) = state.owners.fetch_and_count(&filters).await?;
    Ok(Json(DataTotal { data, total }))
}
