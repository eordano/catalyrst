use axum::extract::{Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::ports::places::{PlaceListFilters, PlaceOrderBy};
use crate::AppState;

fn parse_filters(pairs: &[(String, String)]) -> (PlaceListFilters, bool) {
    let get = |k: &str| pairs.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
    let get_all = |k: &str| {
        pairs
            .iter()
            .filter(|(p, _)| p == k)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    let only_favorites = get("only_favorites")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false);
    let limit = get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(0, 100);
    let offset = get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let only_worlds = get("only_worlds")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false);
    let only_places = get("only_places")
        .map(|v| matches!(v.as_str(), "true" | "1"))
        .unwrap_or(false);
    let f = PlaceListFilters {
        limit,
        offset,
        positions: get_all("pointer"),
        categories: get_all("categories"),
        names: get_all("names"),
        only_highlighted: get("only_highlighted")
            .map(|v| matches!(v.as_str(), "true" | "1"))
            .unwrap_or(false),
        search: get("search"),
        creator_address: get("creator_address").map(|s| s.to_lowercase()),
        sdk: get("sdk"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_worlds,
        only_places,
        ..Default::default()
    };
    (f, only_favorites)
}

fn to_destination(place: &crate::ports::places::PlaceRow) -> Value {
    let mut v = serde_json::to_value(place).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.remove("world_id");
    }
    v
}

pub async fn get_destinations_list(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Value>, ApiError> {
    let (mut filters, only_favorites) = parse_filters(&pairs);
    if only_favorites {
        return Ok(Json(json!({ "ok": true, "data": [], "total": 0 })));
    }
    if let Some(owner) = pairs.iter().find(|(k, _)| k == "owner").map(|(_, v)| v) {
        filters.operated_positions = state.places.operated_positions(owner).await?;
    }
    let (data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    let out: Vec<Value> = data.iter().map(to_destination).collect();
    Ok(Json(json!({ "ok": true, "data": out, "total": total })))
}

pub async fn post_destinations_list_by_id(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let ids = body
        .as_array()
        .ok_or_else(|| {
            ApiError::bad_request("Invalid request body. Expected an array of destination IDs.")
        })?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    if ids.len() > 100 {
        return Err(ApiError::bad_request(
            "Cannot request more than 100 destinations at once",
        ));
    }
    let (mut filters, only_favorites) = parse_filters(&pairs);
    if only_favorites {
        return Ok(Json(json!({ "ok": true, "data": [], "total": 0 })));
    }
    filters.ids = ids;
    let (data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    let out: Vec<Value> = data.iter().map(to_destination).collect();
    Ok(Json(json!({ "ok": true, "data": out, "total": total })))
}
