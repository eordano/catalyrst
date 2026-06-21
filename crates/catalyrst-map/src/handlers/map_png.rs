use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::cache;
use crate::map::TileType;
use crate::render::{render_estate_minimap, render_minimap, render_png, Coord};
use crate::AppState;

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

            let mut selected: Vec<Coord> = data
                .tiles
                .values()
                .filter(|t| t.tile_type == TileType::Owned)
                .filter(|t| t.estate_id.as_deref() == Some(estate_id.as_str()))
                .map(|t| Coord { x: t.x, y: t.y })
                .collect();
            if selected.is_empty() {
                selected = data
                    .tiles
                    .values()
                    .filter(|t| t.estate_id.as_deref() == Some(estate_id.as_str()))
                    .map(|t| Coord { x: t.x, y: t.y })
                    .collect();
            }

            if selected.is_empty() {
                return Err(Response::builder()
                    .status(StatusCode::FOUND)
                    .header(
                        header::LOCATION,
                        std::env::var("DISSOLVED_ESTATE_URL")
                            .ok()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| {
                                "https://ui.decentraland.org/dissolved_estate.png".to_string()
                            }),
                    )
                    .body(Body::empty())
                    .unwrap());
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
