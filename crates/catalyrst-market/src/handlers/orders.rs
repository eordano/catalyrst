use axum::extract::{Query, State};
use axum::Json;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
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

pub const OPEN_BY_ITEMS_MAX_ITEMS: usize = 200;
pub const OPEN_BY_ITEMS_DEFAULT_PER_CONTRACT: i64 = 500;
pub const OPEN_BY_ITEMS_MAX_PER_CONTRACT: i64 = 5000;

/// `GET /v1/orders/open-by-items?item=<contract>-<itemId>&item=...&perContract=N`: the open
/// orders of each `(contract, item)` pair that a cheapest-first page walk of the contract
/// would reach within its first `perContract` rows, in one query.
pub async fn get_open_orders_by_items(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotalString<Order>>, ApiError> {
    let (items, per_contract) = parse_open_by_items(&pairs)?;
    let data = state
        .orders
        .get_open_orders_by_items(&items, per_contract)
        .await?;
    Ok(Json(DataTotalString {
        total: data.len().to_string(),
        data,
    }))
}

pub(crate) fn parse_open_by_items(
    pairs: &[(String, String)],
) -> Result<(Vec<(String, String)>, i64), InvalidParameterError> {
    let p = Params::new(pairs);
    let raw = p.get_list("item", &[]);
    if raw.is_empty() {
        return Err(InvalidParameterError::new("item", ""));
    }
    if raw.len() > OPEN_BY_ITEMS_MAX_ITEMS {
        return Err(InvalidParameterError::new(
            "item",
            format!("{} items (max {OPEN_BY_ITEMS_MAX_ITEMS})", raw.len()),
        ));
    }
    let mut items = Vec::with_capacity(raw.len());
    for entry in &raw {
        let Some((contract, item_id)) = entry.trim().split_once('-') else {
            return Err(InvalidParameterError::new("item", entry.clone()));
        };
        let contract = contract.to_ascii_lowercase();
        let is_addr = contract
            .strip_prefix("0x")
            .is_some_and(|hex| hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
        let is_item = !item_id.is_empty()
            && item_id.len() <= 77
            && item_id.bytes().all(|b| b.is_ascii_digit());
        if !is_addr || !is_item {
            return Err(InvalidParameterError::new("item", entry.clone()));
        }
        items.push((contract, item_id.to_string()));
    }
    let per_contract = p
        .get_number("perContract", None)
        .map(|v| v as i64)
        .unwrap_or(OPEN_BY_ITEMS_DEFAULT_PER_CONTRACT)
        .clamp(1, OPEN_BY_ITEMS_MAX_PER_CONTRACT);
    Ok((items, per_contract))
}

#[cfg(test)]
mod open_by_items_tests {
    use super::parse_open_by_items;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_repeated_items_and_the_window() {
        let (items, per) = parse_open_by_items(&pairs(&[
            ("item", "0x016A61FEB6377239E34425B82E5C4B367E52457F-12"),
            ("item", "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-0"),
            ("perContract", "500"),
        ]))
        .unwrap();
        assert_eq!(
            items,
            vec![
                (
                    "0x016a61feb6377239e34425b82e5c4b367e52457f".to_string(),
                    "12".to_string()
                ),
                (
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    "0".to_string()
                ),
            ]
        );
        assert_eq!(per, 500);
    }

    #[test]
    fn window_defaults_and_clamps() {
        let one = pairs(&[("item", "0x016a61feb6377239e34425b82e5c4b367e52457f-1")]);
        assert_eq!(parse_open_by_items(&one).unwrap().1, 500);
        let big = pairs(&[
            ("item", "0x016a61feb6377239e34425b82e5c4b367e52457f-1"),
            ("perContract", "999999"),
        ]);
        assert_eq!(parse_open_by_items(&big).unwrap().1, 5000);
        let zero = pairs(&[
            ("item", "0x016a61feb6377239e34425b82e5c4b367e52457f-1"),
            ("perContract", "0"),
        ]);
        assert_eq!(parse_open_by_items(&zero).unwrap().1, 1);
    }

    #[test]
    fn rejects_malformed_items() {
        for bad in [
            "",
            "0x016a61feb6377239e34425b82e5c4b367e52457f",
            "0x016a61feb6377239e34425b82e5c4b367e52457f-",
            "0x016a61feb6377239e34425b82e5c4b367e52457f-abc",
            "0x016a-12",
            "016a61feb6377239e34425b82e5c4b367e52457f-12",
        ] {
            assert!(
                parse_open_by_items(&pairs(&[("item", bad)])).is_err(),
                "{bad:?} must be rejected"
            );
        }
        assert!(parse_open_by_items(&[]).is_err(), "no items is an error");
    }
}
