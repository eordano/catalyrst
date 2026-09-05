use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::cache;
use crate::map::MapData;
use crate::render::{render_estate_minimap, render_minimap, render_png, Coord};
use crate::AppState;

// Thread-local so parallel test threads do not cross-count.
#[cfg(test)]
thread_local! {
    pub(crate) static ESTATE_SELECT_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Estate parcel selection through the prebuilt index, replacing two full-map scans. Prefers
/// owned parcels and falls back to all parcels carrying the estate id, as the scans did.
fn select_estate_coords(data: &MapData, estate_id: &str) -> Vec<Coord> {
    let coords = data
        .estates_owned
        .get(estate_id)
        .filter(|v| !v.is_empty())
        .or_else(|| data.estates_all.get(estate_id));
    match coords {
        Some(v) => {
            #[cfg(test)]
            ESTATE_SELECT_VISITS.with(|c| c.set(c.get() + v.len()));
            v.iter().map(|&(x, y)| Coord { x, y }).collect()
        }
        None => Vec::new(),
    }
}

struct Params {
    width: u32,
    height: u32,
    size: u32,
    center: Coord,
    show_on_sale: bool,
    show_on_rent: bool,
    selected: Vec<Coord>,
}

fn parse_clamped(q: &HashMap<String, String>, name: &str, default: i64, min: i64, max: i64) -> i64 {
    let v = q
        .get(name)
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(default);
    v.clamp(min, max)
}

fn extract_params(q: &HashMap<String, String>) -> Params {
    let width = parse_clamped(q, "width", 1024, 100, 4096) as u32;
    let height = parse_clamped(q, "height", 1024, 100, 4096) as u32;
    let size = parse_clamped(q, "size", 20, 5, 50) as u32;

    let center = q
        .get("center")
        .and_then(|s| parse_coord(s))
        .unwrap_or(Coord { x: 0, y: 0 });

    let show_on_sale = q.get("on-sale").map(|s| s == "true").unwrap_or(false);
    let show_on_rent = q
        .get("listed-for-rent")
        .map(|s| s == "true")
        .unwrap_or(false);

    let selected = q
        .get("selected")
        .map(|s| s.split(';').filter_map(parse_coord).collect::<Vec<_>>())
        .unwrap_or_default();

    Params {
        width,
        height,
        size,
        center,
        show_on_sale,
        show_on_rent,
        selected,
    }
}

impl Params {
    fn key_suffix(&self) -> String {
        let mut sel: Vec<(i32, i32)> = self.selected.iter().map(|c| (c.x, c.y)).collect();
        sel.sort_unstable();
        let sel: Vec<String> = sel.iter().map(|(x, y)| format!("{x},{y}")).collect();
        format!(
            "w{}h{}s{}c{},{}os{}or{}sel{}",
            self.width,
            self.height,
            self.size,
            self.center.x,
            self.center.y,
            self.show_on_sale as u8,
            self.show_on_rent as u8,
            sel.join(";"),
        )
    }
}

fn cached_png_render<F>(
    state: &AppState,
    headers: &HeaderMap,
    key: String,
    last: i64,
    max_age: u32,
    swr: u32,
    render: F,
) -> Response
where
    F: FnOnce() -> Result<Vec<u8>, Response>,
{
    let etag = cache::etag_for(last, &key);
    if let Some(r) = cache::not_modified_etag(headers, last, &etag, max_age, swr) {
        return r;
    }
    let bytes = match state.map.cached_png(&key) {
        Some(b) => b,
        None => match render() {
            Ok(b) => {
                let b = Arc::new(b);
                state.map.store_png(key, b.clone());
                b
            }
            Err(resp) => return finalize(resp, last, max_age, swr),
        },
    };
    let mut resp = png_response_arc(bytes);
    cache::apply_etag(&mut resp, last, &etag, max_age, swr);
    resp
}

fn render_error(e: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "ok": false, "error": e })),
    )
        .into_response()
}

fn parse_coord(s: &str) -> Option<Coord> {
    let mut parts = s.split(',');
    let x = parts.next()?.trim().parse::<i32>().ok()?;
    let y = parts.next()?.trim().parse::<i32>().ok()?;
    Some(Coord { x, y })
}

fn png_response_arc(bytes: Arc<Vec<u8>>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png")],
        Body::from(bytes.as_ref().clone()),
    )
        .into_response()
}

fn not_ready() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, "Not ready").into_response()
}

pub async fn map_png(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let p = extract_params(&q);
    let key = format!("map:{}", p.key_suffix());
    cached_png_render(
        &state,
        &headers,
        key,
        last,
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
        || {
            let data = state.map.snapshot().ok_or_else(not_ready)?;
            render_png(
                &data,
                p.width,
                p.height,
                p.size,
                p.center,
                &p.selected,
                p.show_on_sale,
                p.show_on_rent,
            )
            .map_err(render_error)
        },
    )
}

fn finalize(mut resp: Response, last: i64, max_age: u32, swr: u32) -> Response {
    cache::apply(&mut resp, last, max_age, swr);
    resp
}

pub async fn parcel_map_png(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((x, y)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let p = extract_params(&q);
    let center = Coord {
        x: x.parse().unwrap_or(0),
        y: y.parse().unwrap_or(0),
    };
    let selected = vec![center];
    let key = format!("parcel:{},{}:{}", center.x, center.y, p.key_suffix());
    cached_png_render(
        &state,
        &headers,
        key,
        last,
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
        || {
            let data = state.map.snapshot().ok_or_else(not_ready)?;
            render_png(
                &data,
                p.width,
                p.height,
                p.size,
                center,
                &selected,
                p.show_on_sale,
                p.show_on_rent,
            )
            .map_err(render_error)
        },
    )
}

pub async fn estate_map_png(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(estate_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let last = state.map.last_updated_at();
    let p = extract_params(&q);
    let key = format!("estate:{}:{}", estate_id, p.key_suffix());
    cached_png_render(
        &state,
        &headers,
        key,
        last,
        cache::DEFAULT_MAX_AGE,
        cache::DEFAULT_SWR,
        || {
            let data = state.map.snapshot().ok_or_else(not_ready)?;

            let selected: Vec<Coord> = select_estate_coords(&data, &estate_id);

            if selected.is_empty() {
                let placeholder = std::env::var("DISSOLVED_ESTATE_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                return Err(match placeholder {
                    Some(url) => Response::builder()
                        .status(StatusCode::FOUND)
                        .header(header::LOCATION, url)
                        .body(Body::empty())
                        .unwrap(),
                    None => Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                });
            }

            let mut xs: Vec<i32> = selected.iter().map(|c| c.x).collect();
            let mut ys: Vec<i32> = selected.iter().map(|c| c.y).collect();
            xs.sort_unstable();
            ys.sort_unstable();
            let center = Coord {
                x: xs[xs.len() / 2],
                y: ys[ys.len() / 2],
            };

            render_png(
                &data,
                p.width,
                p.height,
                p.size,
                center,
                &selected,
                p.show_on_sale,
                p.show_on_rent,
            )
            .map_err(render_error)
        },
    )
}

pub async fn minimap_png(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let last = state.map.last_updated_at();
    cached_png_render(
        &state,
        &headers,
        "minimap".to_string(),
        last,
        cache::MINIMAP_MAX_AGE,
        cache::MINIMAP_SWR,
        || {
            let data = state.map.snapshot().ok_or_else(not_ready)?;
            render_minimap(&data).map_err(render_error)
        },
    )
}

pub async fn estate_minimap_png(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let last = state.map.last_updated_at();
    cached_png_render(
        &state,
        &headers,
        "estatemap".to_string(),
        last,
        cache::MINIMAP_MAX_AGE,
        cache::MINIMAP_SWR,
        || {
            let data = state.map.snapshot().ok_or_else(not_ready)?;
            render_estate_minimap(&data).map_err(render_error)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Tile, TileType};
    use std::collections::HashMap;

    fn tile(x: i32, y: i32, estate_id: Option<&str>, owned: bool) -> Tile {
        Tile {
            id: format!("{x},{y}"),
            x,
            y,
            nft_id: None,
            tile_type: if owned {
                TileType::Owned
            } else {
                TileType::Unowned
            },
            top: false,
            left: false,
            top_left: false,
            updated_at: 0,
            name: None,
            owner: None,
            estate_id: estate_id.map(|s| s.to_string()),
            token_id: None,
            price: None,
            expires_at: None,
            rental_listing: None,
        }
    }

    fn build_map_data(n: usize, estate_id: &str, estate_size: usize) -> MapData {
        let mut tiles: HashMap<String, Tile> = HashMap::new();
        let mut estates_owned: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        let mut estates_all: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        for i in 0..estate_size as i32 {
            let t = tile(i, 0, Some(estate_id), true);
            estates_all
                .entry(estate_id.into())
                .or_default()
                .push((i, 0));
            estates_owned
                .entry(estate_id.into())
                .or_default()
                .push((i, 0));
            tiles.insert(t.id.clone(), t);
        }
        for j in estate_size..n {
            let x = 1000 + j as i32;
            let t = tile(x, 0, None, false);
            tiles.insert(t.id.clone(), t);
        }
        MapData {
            tiles,
            last_updated_at: 0,
            estates_owned,
            estates_all,
        }
    }

    // The two full-map scans visited 5000; the index visits 4.
    #[test]
    fn estate_lookup_visits_estate_size_not_map() {
        let n = 5000;
        let k = 4;
        let data = build_map_data(n, "77", k);

        ESTATE_SELECT_VISITS.with(|c| c.set(0));
        let selected = select_estate_coords(&data, "77");
        let visits = ESTATE_SELECT_VISITS.with(|c| c.get());

        assert_eq!(
            visits, k,
            "must visit only the estate's parcels, not all tiles"
        );

        let mut got: Vec<(i32, i32)> = selected.iter().map(|c| (c.x, c.y)).collect();
        got.sort_unstable();
        let mut want: Vec<(i32, i32)> = data
            .tiles
            .values()
            .filter(|t| t.tile_type == TileType::Owned)
            .filter(|t| t.estate_id.as_deref() == Some("77"))
            .map(|t| (t.x, t.y))
            .collect();
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn estate_lookup_falls_back_to_all_parcels() {
        let mut tiles: HashMap<String, Tile> = HashMap::new();
        let mut estates_all: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        for i in 0..3i32 {
            let t = tile(i, 5, Some("99"), false);
            estates_all.entry("99".into()).or_default().push((i, 5));
            tiles.insert(t.id.clone(), t);
        }
        let data = MapData {
            tiles,
            last_updated_at: 0,
            estates_owned: HashMap::new(),
            estates_all,
        };
        let mut got: Vec<(i32, i32)> = select_estate_coords(&data, "99")
            .iter()
            .map(|c| (c.x, c.y))
            .collect();
        got.sort_unstable();
        assert_eq!(got, vec![(0, 5), (1, 5), (2, 5)]);

        // An absent estate selects nothing, preserving the 404/302 path.
        assert!(select_estate_coords(&data, "does-not-exist").is_empty());
    }
}
