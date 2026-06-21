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

    let fields: Option<Vec<String>> = if let Some(s) = q.get("include") {
        Some(
            s.split(',')
                .filter(|f| VALID_FIELDS.contains(f))
                .map(|s| s.to_string())
                .collect(),
        )
    } else if let Some(s) = q.get("exclude") {
        let excluded: Vec<&str> = s.split(',').collect();
        Some(
            VALID_FIELDS
                .iter()
                .filter(|f| !excluded.contains(f))
                .map(|s| s.to_string())
                .collect(),
        )
    } else {
        None
    };

    let mut out = Vec::new();
    for (id, tile) in tiles {
        if let Some((min_x, max_x, min_y, max_y)) = bbox {
            if tile.x < min_x || tile.x > max_x || tile.y < min_y || tile.y > max_y {
                continue;
            }
        }
        let mut obj = serde_json::to_value(tile).unwrap_or(Value::Null);
        if let Some(fields) = &fields {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rentals::{RentalPeriod, TileRentalListing};

    fn owned_tile() -> Tile {
        Tile {
            id: "10,20".into(),
            x: 10,
            y: 20,
            nft_id: Some("0xland-123".into()),
            tile_type: TileType::Owned,
            top: true,
            left: false,
            top_left: false,
            updated_at: 1700,
            name: Some("Cool Parcel".into()),
            owner: Some("0xowner".into()),
            estate_id: Some("42".into()),
            token_id: Some("123".into()),
            price: Some(100.0),
            expires_at: Some(2000),
            rental_listing: Some(TileRentalListing {
                expiration: 9999,
                periods: vec![
                    RentalPeriod {
                        min_days: 1,
                        max_days: 7,
                        price_per_day: "100".into(),
                    },
                    RentalPeriod {
                        min_days: 8,
                        max_days: 30,
                        price_per_day: "2000".into(),
                    },
                ],
                updated_at: 1700,
            }),
        }
    }

    fn keys(v: &Value) -> Vec<String> {
        v.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn v2_tile_shape_matches_upstream() {
        let v = serde_json::to_value(owned_tile()).unwrap();
        let mut got = keys(&v);
        got.sort();
        let mut want = vec![
            "id",
            "x",
            "y",
            "nftId",
            "type",
            "top",
            "left",
            "topLeft",
            "updatedAt",
            "name",
            "owner",
            "estateId",
            "tokenId",
            "price",
            "expiresAt",
            "rentalListing",
        ];
        want.sort();
        assert_eq!(got, want);

        for forbidden in ["rentalPricePerDay", "rentalExpiresAt", "estateSize"] {
            assert!(
                v.get(forbidden).is_none(),
                "v2 tile must not carry flat `{forbidden}`"
            );
        }

        let rl = v.get("rentalListing").unwrap().as_object().unwrap();
        let mut rl_keys: Vec<&String> = rl.keys().collect();
        rl_keys.sort();
        assert_eq!(rl_keys, vec!["expiration", "periods", "updatedAt"]);
        assert_eq!(v["type"], "owned");
        assert_eq!(v["topLeft"], false);
    }

    #[test]
    fn unowned_tile_omits_optional_fields() {
        let tile = Tile {
            id: "0,0".into(),
            x: 0,
            y: 0,
            nft_id: None,
            tile_type: TileType::Unowned,
            top: false,
            left: false,
            top_left: false,
            updated_at: 0,
            name: None,
            owner: None,
            estate_id: None,
            token_id: None,
            price: None,
            expires_at: None,
            rental_listing: None,
        };
        let v = serde_json::to_value(tile).unwrap();
        for absent in [
            "nftId",
            "name",
            "owner",
            "estateId",
            "tokenId",
            "price",
            "expiresAt",
            "rentalListing",
        ] {
            assert!(v.get(absent).is_none(), "`{absent}` should be omitted");
        }
        assert_eq!(v["type"], "unowned");
    }

    #[test]
    fn legacy_tile_carries_rental_price_per_day() {
        let v = to_legacy(&owned_tile());
        assert_eq!(v["rentalPricePerDay"], json!("2000"));

        assert_eq!(v["type"], json!(10));
        assert_eq!(v["estate_id"], json!("42"));
        assert_eq!(v["top"], json!(1));
        assert!(v.get("left").is_none());

        assert!(v.get("rentalListing").is_none());
    }

    #[test]
    fn include_projects_only_valid_fields() {
        let mut tiles = HashMap::new();
        tiles.insert("10,20".to_string(), owned_tile());
        let q: HashMap<String, String> = [(
            "include".to_string(),
            "id,x,y,owner,rentalListing,bogus".to_string(),
        )]
        .into_iter()
        .collect();
        let out = filter_tiles(&tiles, &q);
        assert_eq!(out.len(), 1);
        let mut got = keys(&out[0].1);
        got.sort();
        assert_eq!(got, vec!["id", "owner", "x", "y"]);
    }

    #[test]
    fn exclude_keeps_remaining_valid_fields() {
        let mut tiles = HashMap::new();
        tiles.insert("10,20".to_string(), owned_tile());
        let q: HashMap<String, String> =
            [("exclude".to_string(), "name,price,expiresAt".to_string())]
                .into_iter()
                .collect();
        let out = filter_tiles(&tiles, &q);
        let v = &out[0].1;

        for f in ["name", "price", "expiresAt"] {
            assert!(v.get(f).is_none(), "`{f}` should be excluded");
        }

        assert_eq!(v["owner"], json!("0xowner"));

        assert!(v.get("rentalListing").is_none());
        assert!(v.get("nftId").is_none());
    }

    #[test]
    fn bbox_filters_out_of_range_tiles() {
        let mut tiles = HashMap::new();
        tiles.insert("10,20".to_string(), owned_tile());
        let mut t2 = owned_tile();
        t2.id = "100,100".into();
        t2.x = 100;
        t2.y = 100;
        tiles.insert("100,100".to_string(), t2);
        let q: HashMap<String, String> = [
            ("x1".to_string(), "0".to_string()),
            ("x2".to_string(), "50".to_string()),
            ("y1".to_string(), "0".to_string()),
            ("y2".to_string(), "50".to_string()),
        ]
        .into_iter()
        .collect();
        let out = filter_tiles(&tiles, &q);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "10,20");
    }
}
