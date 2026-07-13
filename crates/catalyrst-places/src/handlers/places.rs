use axum::extract::{Path, Query, State};
use axum::Json;

use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{PlaceListFilters, PlaceOrderBy, PlaceRow, PlaceStatusRow};
use crate::AppState;

pub async fn get_place(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
    Path(place_id): Path<String>,
) -> Result<Json<ApiData<PlaceRow>>, ApiError> {
    match state.places.find_by_id(&place_id).await? {
        Some(mut p) => {
            let user = crate::auth::auth_address_optional(&headers);
            state
                .places
                .apply_user_interactions(user.as_deref(), std::slice::from_mut(&mut p))
                .await;
            p.apply_realms_detail(with_realms_detail(&pairs));
            Ok(Json(ApiData::ok(p)))
        }
        None => Err(ApiError::not_found(format!(
            "Not found place \"{}\"",
            place_id
        ))),
    }
}

pub async fn get_place_list(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<ApiDataTotal<PlaceRow>>, ApiError> {
    let mut filters = parse_filters(&pairs);

    let user = crate::auth::auth_address_optional(&headers);
    let only_favorites = pairs
        .iter()
        .any(|(k, v)| k == "only_favorites" && matches!(v.as_str(), "true" | "1"));
    if only_favorites {
        match &user {
            None => return Ok(Json(ApiDataTotal::ok(vec![], 0))),
            Some(addr) => match state.places.favorite_entity_ids(addr).await? {
                Some(ids) if !ids.is_empty() => filters.ids = ids,
                _ => return Ok(Json(ApiDataTotal::ok(vec![], 0))),
            },
        }
    }

    if let Some(owner) = pairs.iter().find(|(k, _)| k == "owner").map(|(_, v)| v) {
        filters.owner_filtered = true;
        filters.operated_positions = state.places.operated_positions(owner).await?;
    }

    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    state
        .places
        .apply_user_interactions(user.as_deref(), &mut data)
        .await;
    let realms = with_realms_detail(&pairs);
    for row in &mut data {
        row.apply_realms_detail(realms);
    }
    Ok(Json(ApiDataTotal::ok(data, total)))
}

pub async fn post_place_list_by_id(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(ids): Json<serde_json::Value>,
) -> Result<Json<ApiDataTotal<PlaceRow>>, ApiError> {
    let ids = ids
        .as_array()
        .ok_or_else(|| {
            ApiError::bad_request("Invalid request body. Expected an array of place IDs.")
        })?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    if ids.len() > 100 {
        return Err(ApiError::bad_request(
            "Cannot request more than 100 places at once",
        ));
    }
    let (mut data, total) = tokio::try_join!(
        state.places.find_by_ids(&ids),
        state.places.count_by_ids(&ids),
    )?;
    let user = crate::auth::auth_address_optional(&headers);
    state
        .places
        .apply_user_interactions(user.as_deref(), &mut data)
        .await;
    Ok(Json(ApiDataTotal::ok(data, total)))
}

pub async fn post_place_status_list_by_id(
    State(state): State<AppState>,
    Json(ids): Json<serde_json::Value>,
) -> Result<Json<ApiDataTotal<PlaceStatusRow>>, ApiError> {
    let ids = ids
        .as_array()
        .ok_or_else(|| {
            ApiError::bad_request("Invalid request body. Expected an array of place IDs.")
        })?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    if ids.len() > 100 {
        return Err(ApiError::bad_request(
            "Cannot request more than 100 places at once",
        ));
    }
    let (data, total) = tokio::try_join!(
        state.places.find_by_ids_status(&ids),
        state.places.count_by_ids(&ids),
    )?;
    Ok(Json(ApiDataTotal::ok(data, total)))
}

pub fn with_realms_detail(pairs: &[(String, String)]) -> bool {
    pairs
        .iter()
        .any(|(k, v)| k == "with_realms_detail" && matches!(v.as_str(), "true" | "1"))
}

fn parse_filters(pairs: &[(String, String)]) -> PlaceListFilters {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let bool_q = |k: &str| {
        get(k)
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
            .unwrap_or(false)
    };
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let order_by = PlaceOrderBy::parse(get("order_by").as_deref());
    let order_desc = !matches!(get("order").as_deref(), Some("asc"));
    PlaceListFilters {
        limit,
        offset,
        positions: get_all("positions"),
        names: get_all("names"),
        categories: get_all("categories"),
        only_highlighted: bool_q("only_highlighted"),
        search: get("search"),
        creator_address: get("creator_address").map(|s| s.to_lowercase()),
        sdk: get("sdk"),
        order_by,
        order_desc,
        ids: Vec::new(),
        only_worlds: false,
        only_places: true,
        operated_positions: Vec::new(),
        owner_filtered: false,
        destinations_mode: false,
        place_user_counts: Vec::new(),
        world_user_counts: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(kvs: &[(&str, &str)]) -> Vec<(String, String)> {
        kvs.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn with_realms_detail_is_truthy_only() {
        assert!(with_realms_detail(&pairs(&[(
            "with_realms_detail",
            "true"
        )])));
        assert!(with_realms_detail(&pairs(&[("with_realms_detail", "1")])));
        assert!(!with_realms_detail(&pairs(&[(
            "with_realms_detail",
            "false"
        )])));
        assert!(!with_realms_detail(&pairs(&[("with_realms_detail", "0")])));
        assert!(!with_realms_detail(&pairs(&[])));
    }

    #[test]
    fn parse_filters_reads_residual_filters() {
        let f = parse_filters(&pairs(&[
            ("names", "Foo.dcl.eth"),
            ("names", "Bar.dcl.eth"),
            ("sdk", "7"),
            ("creator_address", "0xABC"),
            ("only_highlighted", "true"),
            ("positions", "1,2"),
        ]));
        assert_eq!(f.names, vec!["Foo.dcl.eth", "Bar.dcl.eth"]);
        assert_eq!(f.sdk.as_deref(), Some("7"));
        assert_eq!(f.creator_address.as_deref(), Some("0xabc"));
        assert!(f.only_highlighted);
        assert_eq!(f.positions, vec!["1,2"]);
    }
}
