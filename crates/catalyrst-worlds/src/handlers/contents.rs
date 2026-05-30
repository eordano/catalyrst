use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::http::ApiError;
use crate::AppState;

const FORWARD_REQ_HEADERS: &[&str] = &["range", "if-none-match", "if-modified-since"];

const EXPOSED_HEADERS: &str = "ETag, Accept-Ranges, Content-Range";

const IMMUTABLE_CACHE_CONTROL: &str = "public,max-age=31536000,s-maxage=31536000,immutable";

fn is_ipfs_v2(hash: &str) -> bool {
    hash.len() == 59
        && hash.starts_with("ba")
        && hash.bytes().all(|b| b.is_ascii_alphanumeric())
}

const FORWARD_RESP_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "cache-control",
];

pub async fn get_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::GET).await
}

pub async fn head_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::HEAD).await
}

async fn proxy(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
    method: Method,
) -> Result<Response, ApiError> {
    if !is_ipfs_v2(&hash) {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    let local = state.cfg.contents_dir.join(&hash);
    if let Ok(meta) = tokio::fs::metadata(&local).await {
        if meta.is_file() {
            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/octet-stream")
                .header("content-length", meta.len())
                .header("etag", format!("\"{hash}\""))
                .header("cache-control", IMMUTABLE_CACHE_CONTROL)
                .header("access-control-expose-headers", EXPOSED_HEADERS);
            if method == Method::HEAD {
                return Ok(builder.body(Body::empty()).unwrap());
            }
            let bytes = tokio::fs::read(&local)
                .await
                .map_err(|e| ApiError::internal(format!("local content read: {e}")))?;
            builder = builder.header("accept-ranges", "none");
            return Ok(builder.body(Body::from(bytes)).unwrap());
        }
    }

    let url = format!("{}/contents/{}", state.cfg.contents_upstream_url, hash);

    let mut req = match method {
        Method::HEAD => state.http.head(&url),
        _ => state.http.get(&url),
    };
    for name in FORWARD_REQ_HEADERS {
        if let Some(v) = headers.get(*name) {
            req = req.header(*name, v);
        }
    }

    let upstream = req
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("contents upstream error: {e}")))?;

    let status = StatusCode::from_u16(upstream.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);

    let mut out_headers = HeaderMap::new();
    for name in FORWARD_RESP_HEADERS {
        if let Some(v) = upstream.headers().get(*name) {
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                out_headers.insert(hn, hv);
            }
        }
    }

    if status.is_success() {
        out_headers.insert(
            HeaderName::from_static("access-control-expose-headers"),
            HeaderValue::from_static(EXPOSED_HEADERS),
        );
        if !out_headers.contains_key("cache-control") {
            out_headers.insert(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static(IMMUTABLE_CACHE_CONTROL),
            );
        }
        if !out_headers.contains_key("etag") {
            if let Ok(hv) = HeaderValue::from_str(&format!("\"{hash}\"")) {
                out_headers.insert(HeaderName::from_static("etag"), hv);
            }
        }
        if !out_headers.contains_key("accept-ranges") {
            out_headers.insert(
                HeaderName::from_static("accept-ranges"),
                HeaderValue::from_static("bytes"),
            );
        }
    }

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        let stream = upstream.bytes_stream();
        Body::from_stream(stream)
    };

    let mut response = (status, body).into_response();
    response.headers_mut().extend(out_headers);
    Ok(response)
}
