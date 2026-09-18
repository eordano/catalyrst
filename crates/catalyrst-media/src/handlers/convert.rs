use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use catalyrst_commons::http::read_body_capped;
use serde::Deserialize;

use crate::AppState;

const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
static THUMBNAIL_REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);

#[derive(Deserialize)]
pub struct ConvertParams {
    pub url: String,
    pub width: Option<u32>,
}

fn build_response(status: StatusCode, content_type: &str, body: Bytes) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (status, headers, body).into_response()
}

fn cached_response(hit: (u16, String, Bytes)) -> Response {
    let (status, content_type, body) = hit;
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    build_response(status, &content_type, body)
}

pub async fn convert(State(state): State<AppState>, Query(p): Query<ConvertParams>) -> Response {
    if p.width
        .is_some_and(|width| !super::thumbnail::valid_width(width))
    {
        return (StatusCode::BAD_REQUEST, "width must be 320, 640 or 960").into_response();
    }
    let cache_key = p.width.map_or_else(
        || p.url.clone(),
        |width| format!("thumbnail:{width}:{}", p.url),
    );
    if let Some(hit) = state.convert_cache_get(&cache_key) {
        return cached_response(hit);
    }

    let mut current = match reqwest::Url::parse(&p.url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return (StatusCode::BAD_REQUEST, "url must be http(s)").into_response(),
    };

    let gate = state.convert_gate(&cache_key);
    let _flight = match gate.gate.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            let guard = gate.gate.lock().await;
            // A follower serves whatever the flight ahead of it cached.
            if let Some(hit) = state.convert_cache_get(&cache_key) {
                return cached_response(hit);
            }
            guard
        }
    };

    let _thumbnail_request = if p.width.is_some() {
        match THUMBNAIL_REQUESTS.try_acquire() {
            Ok(permit) => Some(permit),
            Err(_) => {
                return (StatusCode::SERVICE_UNAVAILABLE, "Thumbnail service busy").into_response()
            }
        }
    } else {
        None
    };

    let mut hops = 0;
    let upstream = loop {
        let Some(pinned) = state.resolve_pinned(&current).await else {
            return (StatusCode::FORBIDDEN, "url host is not publicly routable").into_response();
        };
        let host = current.host_str().unwrap_or_default().to_string();
        let client = match state.pinned_client(&host, pinned) {
            Ok(c) => c,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("client build failed: {e}"))
                    .into_response()
            }
        };
        let resp = match client.get(current.clone()).send().await {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    format!("upstream fetch failed: {e}"),
                )
                    .into_response()
            }
        };
        if resp.status().is_redirection() {
            hops += 1;
            if hops > MAX_REDIRECTS {
                return (StatusCode::BAD_GATEWAY, "too many redirects").into_response();
            }
            let loc = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok());
            let Some(loc) = loc else {
                return (StatusCode::BAD_GATEWAY, "redirect without location").into_response();
            };
            current = match current.join(loc) {
                Ok(u) if matches!(u.scheme(), "http" | "https") => u,
                _ => return (StatusCode::BAD_REQUEST, "invalid redirect target").into_response(),
            };
            continue;
        }
        break resp;
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let max_bytes = if p.width.is_some() {
        8 * 1024 * 1024
    } else {
        MAX_BODY_BYTES
    };
    let body = match read_body_capped(upstream, max_bytes).await {
        Ok(body) => Bytes::from(body),
        Err(e) => {
            return if e.downcast_ref::<reqwest::Error>().is_some() {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("upstream read failed: {e}"),
                )
                    .into_response()
            } else {
                (StatusCode::PAYLOAD_TOO_LARGE, "source too large").into_response()
            }
        }
    };

    if let Some(width) = p.width.filter(|_| status.is_success()) {
        match super::thumbnail::render(body, width).await {
            Ok((kind, bytes)) => {
                state.convert_cache_put(&cache_key, status.as_u16(), kind, bytes.clone());
                return build_response(status, kind, bytes);
            }
            Err(message) => return (StatusCode::UNPROCESSABLE_ENTITY, message).into_response(),
        }
    }
    if status.is_success() {
        state.convert_cache_put(&cache_key, status.as_u16(), &content_type, body.clone());
    }
    build_response(status, &content_type, body)
}
