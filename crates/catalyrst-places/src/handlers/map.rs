use axum::extract::{Query, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::http::errors::ApiError;
use crate::ports::places::{PlaceListFilters, PlaceOrderBy};
use crate::AppState;

fn list_filters(pairs: &[(String, String)], only_worlds: bool) -> (PlaceListFilters, bool) {
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
    let f = PlaceListFilters {
        limit,
        offset,
        positions: get_all("positions"),
        categories: get_all("categories"),
        only_highlighted: get("only_highlighted")
            .map(|v| matches!(v.as_str(), "true" | "1"))
            .unwrap_or(false),
        search: get("search"),
        creator_address: get("creator_address").map(|s| s.to_lowercase()),
        sdk: get("sdk"),
        order_by: PlaceOrderBy::parse(get("order_by").as_deref()),
        order_desc: !matches!(get("order").as_deref(), Some("asc")),
        only_worlds,
        ..Default::default()
    };
    (f, only_favorites)
}

pub async fn get_map_places(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Value>, ApiError> {
    let (mut filters, only_favorites) = list_filters(&pairs, false);
    if only_favorites {
        return Ok(Json(
            json!({ "ok": true, "data": serde_json::Map::new(), "total": 0 }),
        ));
    }
    filters.only_places = !filters.only_highlighted;
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;

    let realms = crate::handlers::places::with_realms_detail(&pairs);
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    for place in &mut data {
        place.apply_realms_detail(realms);
        let key = place.base_position.clone();
        let mut v = serde_json::to_value(&place).unwrap_or(Value::Null);
        if let Some(obj) = v.as_object_mut() {
            obj.remove("positions");
        }
        map.insert(key, v);
    }
    Ok(Json(json!({ "ok": true, "data": map, "total": total })))
}

pub async fn get_all_places_list(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Value>, ApiError> {
    let (filters, only_favorites) = list_filters(&pairs, false);
    if only_favorites {
        return Ok(Json(json!({ "ok": true, "data": [], "total": 0 })));
    }
    let (mut data, total) = tokio::try_join!(
        state.places.find_list(&filters),
        state.places.count_list(&filters),
    )?;
    let realms = crate::handlers::places::with_realms_detail(&pairs);
    for place in &mut data {
        place.apply_realms_detail(realms);
    }
    Ok(Json(json!({ "ok": true, "data": data, "total": total })))
}
