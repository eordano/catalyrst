use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

use crate::cache;
use crate::map::{coords_to_id, LegacyTile, Tile, TileType};
use crate::AppState;

/// Params that `filter_tiles` actually consumes. The cache key is built from
/// exactly these (see `canonical_tiles_key`) so semantically-equal requests
/// collapse to one entry regardless of raw query-string order or extra params.
const KEY_PARAMS: &[&str] = &["x1", "x2", "y1", "y2", "exclude", "include"];

/// Canonical, order-independent cache key derived from the parsed params rather than the raw
/// query string. `include`/`exclude` lists are sorted: projection is key-order-insensitive.
fn canonical_tiles_key(prefix: &str, q: &HashMap<String, String>) -> String {
    let mut s = String::with_capacity(prefix.len() + 32);
    s.push_str(prefix);
    s.push('?');
    for (i, p) in KEY_PARAMS.iter().enumerate() {
        if i > 0 {
            s.push('&');
        }
        s.push_str(p);
        s.push('=');
        if let Some(v) = q.get(*p) {
            if *p == "include" || *p == "exclude" {
                let mut parts: Vec<&str> = v.split(',').collect();
                parts.sort_unstable();
                s.push_str(&parts.join(","));
            } else {
                s.push_str(v);
            }
        }
    }
    s
}

// Thread-local so parallel test threads do not cross-count.
#[cfg(test)]
thread_local! {
    pub(crate) static FILTER_TILE_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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

    // A bbox smaller than the tile set is cheaper to probe coordinate by coordinate than to
    // find by scanning every parcel. Ids are `coords_to_id`, so the visited set is identical.
    let use_bbox = match bbox {
        Some((min_x, max_x, min_y, max_y)) => {
            let w = (max_x as i64 - min_x as i64 + 1) as u64;
            let h = (max_y as i64 - min_y as i64 + 1) as u64;
            w.checked_mul(h)
                .map(|area| area < tiles.len() as u64)
                .unwrap_or(false)
        }
        None => false,
    };

    let mut out = Vec::new();
    if let (true, Some((min_x, max_x, min_y, max_y))) = (use_bbox, bbox) {
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                #[cfg(test)]
                FILTER_TILE_VISITS.with(|c| c.set(c.get() + 1));
                if let Some((id, tile)) = tiles.get_key_value(&coords_to_id(x, y)) {
                    push_projected(id, tile, &fields, &mut out);
                }
            }
        }
    } else {
        for (id, tile) in tiles {
            #[cfg(test)]
            FILTER_TILE_VISITS.with(|c| c.set(c.get() + 1));
            if let Some((min_x, max_x, min_y, max_y)) = bbox {
                if tile.x < min_x || tile.x > max_x || tile.y < min_y || tile.y > max_y {
                    continue;
                }
            }
            push_projected(id, tile, &fields, &mut out);
        }
    }
    out
}

fn push_projected<'a>(
    id: &'a String,
    tile: &Tile,
    fields: &Option<Vec<String>>,
    out: &mut Vec<(&'a String, Value)>,
) {
    let mut obj = serde_json::to_value(tile).unwrap_or(Value::Null);
    if let Some(fields) = fields {
        obj = project_include(&obj, fields);
    }
    out.push((id, obj));
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
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let cache_key = canonical_tiles_key("v2", &q);
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

pub fn to_legacy(tile: &Tile) -> LegacyTile {
    LegacyTile {
        tile_type: legacy_type(tile),
        x: tile.x,
        y: tile.y,
        top: tile.top.then_some(1),
        left: tile.left.then_some(1),
        top_left: tile.top_left.then_some(1),
        owner: tile.owner.clone(),
        name: tile.name.clone(),
        estate_id: tile.estate_id.clone(),
        price: tile.price,
        rental_price_per_day: tile
            .rental_listing
            .as_ref()
            .map(|rl| rl.max_price_per_day()),
    }
}

pub async fn get_legacy_tiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let cache_key = canonical_tiles_key("v1", &q);
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
            map.insert(
                id.clone(),
                serde_json::to_value(to_legacy(t)).unwrap_or(Value::Null),
            );
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
        let v = serde_json::to_value(to_legacy(&owned_tile())).unwrap();
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

    fn grid_tile(x: i32, y: i32) -> Tile {
        let mut t = owned_tile();
        t.id = coords_to_id(x, y);
        t.x = x;
        t.y = y;
        t
    }

    fn bbox_query(x1: i32, x2: i32, y1: i32, y2: i32) -> HashMap<String, String> {
        [
            ("x1".to_string(), x1.to_string()),
            ("x2".to_string(), x2.to_string()),
            ("y1".to_string(), y1.to_string()),
            ("y2".to_string(), y2.to_string()),
        ]
        .into_iter()
        .collect()
    }

    // The full-map scan visits 10_000; the bbox probe visits 25.
    #[test]
    fn bbox_filter_probes_area_not_total() {
        let mut tiles = HashMap::new();
        for x in 0..100 {
            for y in 0..100 {
                tiles.insert(coords_to_id(x, y), grid_tile(x, y));
            }
        }
        assert_eq!(tiles.len(), 10_000);

        let q = bbox_query(0, 4, 0, 4); // area = 5*5 = 25
        FILTER_TILE_VISITS.with(|c| c.set(0));
        let out = filter_tiles(&tiles, &q);
        let visits = FILTER_TILE_VISITS.with(|c| c.get());

        assert_eq!(visits, 25, "bbox must probe area (25), not scan all tiles");

        let mut got: Vec<String> = out.iter().map(|(id, _)| (*id).clone()).collect();
        got.sort();
        let mut want: Vec<String> = (0..=4)
            .flat_map(|x| (0..=4).map(move |y| coords_to_id(x, y)))
            .collect();
        want.sort();
        assert_eq!(got, want);
    }

    // The raw-query key forked semantically-equal requests into separate entries.
    #[tokio::test]
    async fn param_order_collapses_to_one_entry() {
        // extra ignored param must not change the key
        let a = bbox_query(0, 1, 0, 1);
        let mut b = bbox_query(0, 1, 0, 1);
        b.insert("cachebust".to_string(), "xyz".to_string());
        assert_eq!(
            canonical_tiles_key("v2", &a),
            canonical_tiles_key("v2", &b),
            "ignored params must not fork the cache key"
        );

        // include list order must not change the key
        let c: HashMap<String, String> = [("include".to_string(), "x,y,owner".to_string())]
            .into_iter()
            .collect();
        let d: HashMap<String, String> = [("include".to_string(), "owner,y,x".to_string())]
            .into_iter()
            .collect();
        assert_eq!(canonical_tiles_key("v2", &c), canonical_tiles_key("v2", &d));

        // and both requests land on a single cache entry within one epoch
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://u:p@127.0.0.1:5999/db")
            .unwrap();
        let mc = crate::map::MapComponent::new(
            pool,
            "s".into(),
            "0xland".into(),
            "0xestate".into(),
            64,
            64,
        );
        let body = Arc::new(vec![1u8]);
        mc.store_tiles_response(canonical_tiles_key("v2", &a), body.clone());
        mc.store_tiles_response(canonical_tiles_key("v2", &b), body);
        assert_eq!(
            mc.tiles_cache_len(),
            1,
            "equivalent requests must share one entry"
        );
    }
}
