use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, RawQuery, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

use crate::cache;
use crate::map::{Tile, TileType};
use crate::AppState;

fn finalize(mut resp: Response, last: i64) -> Response {
    cache::apply(&mut resp, last, cache::DEFAULT_MAX_AGE, cache::DEFAULT_SWR);
    resp
}

fn finalize_etag(mut resp: Response, last: i64, key: &str) -> Response {
    let etag = cache::etag_for(last, key);
    cache::apply_etag(
        &mut resp,
        last,
        &etag,
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
    );
    resp
}

fn cached_json(body: Arc<Vec<u8>>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body.as_ref().clone(),
    )
        .into_response()
}

const VALID_FIELDS: &[&str] = &[
    "id",
    "x",
    "y",
    "type",
    "name",
    "top",
    "left",
    "topLeft",
    "updatedAt",
    "owner",
    "estateId",
    "tokenId",
    "price",
    "expiresAt",
    "rentalListing",
    "estateSize",
    "rentalPricePerDay",
    "rentalExpiresAt",
];

fn filter_tiles<'a>(
    tiles: &'a HashMap<String, Tile>,
    q: &HashMap<String, String>,
) -> Vec<(&'a String, Value)> {
    let bbox = match (
        q.get("x1").and_then(|s| s.parse::<i32>().ok()),
        q.get("x2").and_then(|s| s.parse::<i32>().ok()),
        q.get("y1").and_then(|s| s.parse::<i32>().ok()),
        q.get("y2").and_then(|s| s.parse::<i32>().ok()),
    ) {
        (Some(x1), Some(x2), Some(y1), Some(y2)) => {
            Some((x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2)))
        }
        _ => None,
    };

    let include: Option<Vec<String>> = q.get("include").map(|s| {
        s.split(',')
            .filter(|f| VALID_FIELDS.contains(f))
            .map(|s| s.to_string())
            .collect()
    });

    let mut out = Vec::new();
    for (id, tile) in tiles {
        if let Some((min_x, max_x, min_y, max_y)) = bbox {
            if tile.x < min_x || tile.x > max_x || tile.y < min_y || tile.y > max_y {
                continue;
            }
        }
        let mut obj = serde_json::to_value(tile).unwrap_or(Value::Null);
        if let Some(fields) = &include {
            obj = project_include(&obj, fields);
        }
        out.push((id, obj));
    }
    out
}

fn project_include(obj: &Value, fields: &[String]) -> Value {
    let mut m = Map::new();
    if let Value::Object(src) = obj {
        for f in fields {
            if let Some(v) = src.get(f) {
                m.insert(f.clone(), v.clone());
            }
        }
    }
    Value::Object(m)
}

pub async fn get_tiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let cache_key = format!("v2?{}", raw.as_deref().unwrap_or(""));
    if let Some(r) = cache::not_modified_etag(
        &headers,
        last,
        &cache::etag_for(last, &cache_key),
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
    ) {
        return r;
    }
    if !state.map.is_ready() {
        return finalize(
            (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response(),
            last,
        );
    }
    if let Some(body) = state.map.cached_tiles_response(&cache_key) {
        return finalize_etag(cached_json(body), last, &cache_key);
    }
    let Some(data) = state.map.snapshot() else {
        return finalize(
            (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response(),
            last,
        );
    };
    let mut map = Map::new();
    for (id, v) in filter_tiles(&data.tiles, &q) {
        map.insert(id.clone(), v);
    }
    let body =
        serde_json::to_vec(&json!({ "ok": true, "data": Value::Object(map) })).unwrap_or_default();
    let body = Arc::new(body);
    state
        .map
        .store_tiles_response(cache_key.clone(), body.clone());
    finalize_etag(cached_json(body), last, &cache_key)
}

fn legacy_type(tile: &Tile) -> i32 {
    if tile.price.is_some() {
        return 10;
    }
    match tile.tile_type {
        TileType::District => 5,
        TileType::Owned => 9,
        TileType::Unowned => 11,
        TileType::Plaza => 8,
        TileType::Road => 7,
    }
}

fn to_legacy(tile: &Tile) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), json!(legacy_type(tile)));
    m.insert("x".into(), json!(tile.x));
    m.insert("y".into(), json!(tile.y));
    if tile.top {
        m.insert("top".into(), json!(1));
    }
    if tile.left {
        m.insert("left".into(), json!(1));
    }
    if tile.top_left {
        m.insert("topLeft".into(), json!(1));
    }
    if let Some(o) = &tile.owner {
        m.insert("owner".into(), json!(o));
    }
    if let Some(n) = &tile.name {
        m.insert("name".into(), json!(n));
    }
    if let Some(e) = &tile.estate_id {
        m.insert("estate_id".into(), json!(e));
    }
    if let Some(p) = tile.price {
        m.insert("price".into(), json!(p));
    }
    if let Some(rl) = &tile.rental_listing {
        m.insert("rentalPricePerDay".into(), json!(rl.max_price_per_day()));
    }
    Value::Object(m)
}

pub async fn get_legacy_tiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let cache_key = format!("v1?{}", raw.as_deref().unwrap_or(""));
    if let Some(r) = cache::not_modified_etag(
        &headers,
        last,
        &cache::etag_for(last, &cache_key),
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
    ) {
        return r;
    }
    if !state.map.is_ready() {
        return finalize(
            (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response(),
            last,
        );
    }
    if let Some(body) = state.map.cached_tiles_response(&cache_key) {
        return finalize_etag(cached_json(body), last, &cache_key);
    }
    let Some(data) = state.map.snapshot() else {
        return finalize(
            (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response(),
            last,
        );
    };
    let filtered = filter_tiles(&data.tiles, &q);
    let mut map = Map::new();
    for (id, _) in filtered {
        if let Some(t) = data.tiles.get(id) {
            map.insert(id.clone(), to_legacy(t));
        }
    }
    let body =
        serde_json::to_vec(&json!({ "ok": true, "data": Value::Object(map) })).unwrap_or_default();
    let body = Arc::new(body);
    state
        .map
        .store_tiles_response(cache_key.clone(), body.clone());
    finalize_etag(cached_json(body), last, &cache_key)
}

pub async fn tiles_info(State(state): State<AppState>) -> Response {
    if !state.map.is_ready() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("cache-control", "no-cache")],
            "Not ready",
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [("cache-control", "no-cache")],
        Json(json!({ "lastUpdatedAt": state.map.last_updated_at() })),
    )
        .into_response()
}
