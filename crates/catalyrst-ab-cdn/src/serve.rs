use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use catalyrst_server::formatters::{parse_range_header, ParsedRange};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::resolver::resolve_with_casing;
use crate::state::AppState;

fn binary_content_type(etag_source: &str) -> &'static str {
    if etag_source.ends_with(".manifest") {
        "text/cache-manifest"
    } else {
        "application/wasm"
    }
}

fn immutable_cache_control(is_brotli: bool) -> &'static str {
    if is_brotli {
        "public,no-transform,max-age=31536000,immutable"
    } else {
        "public,max-age=31536000,immutable"
    }
}

fn apply_cors(resp: &mut Response) {
    let h = resp.headers_mut();
    h.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    h.insert(
        "Access-Control-Expose-Headers",
        "ETag, Content-Range, Accept-Ranges, Content-Length, Content-Encoding"
            .parse()
            .unwrap(),
    );
    h.insert(
        "Access-Control-Allow-Methods",
        "GET, HEAD, OPTIONS".parse().unwrap(),
    );
}

fn not_found() -> Response {
    let mut resp = (StatusCode::NOT_FOUND, "not found").into_response();
    apply_cors(&mut resp);
    resp
}

async fn resolve(state: &AppState, key: &str, exact: &Path) -> Option<(PathBuf, u64)> {
    if let Some(hit) = state.resolve_cache.get(key).await {
        return hit;
    }
    let exact = exact.to_path_buf();
    let resolved = tokio::task::spawn_blocking(move || {
        let path = resolve_with_casing(&exact)?;
        let size = std::fs::metadata(&path).ok()?.len();
        Some((path, size))
    })
    .await
    .ok()
    .flatten();
    state
        .resolve_cache
        .insert(key.to_string(), resolved.clone())
        .await;
    resolved
}

async fn read_range(path: &Path, start: u64, end: u64) -> Option<Vec<u8>> {
    let mut f = tokio::fs::File::open(path).await.ok()?;
    f.seek(SeekFrom::Start(start)).await.ok()?;
    let len = (end - start + 1) as usize;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

async fn stream_body(path: &Path) -> Option<Body> {
    let f = tokio::fs::File::open(path).await.ok()?;
    Some(Body::from_stream(ReaderStream::new(f)))
}

pub async fn serve_manifest(
    state: &AppState,
    key: &str,
    exact: &Path,
    method: &Method,
) -> Response {
    let Some((path, size)) = resolve(state, key, exact).await else {
        return not_found();
    };

    let mut resp = if *method == Method::HEAD {
        StatusCode::OK.into_response()
    } else {
        match stream_body(&path).await {
            Some(body) => (StatusCode::OK, body).into_response(),
            None => return not_found(),
        }
    };
    let h = resp.headers_mut();
    h.insert("Content-Type", "application/json".parse().unwrap());
    h.insert(
        "Cache-Control",
        "private, max-age=0, no-cache".parse().unwrap(),
    );
    h.insert("Content-Length", size.to_string().parse().unwrap());
    apply_cors(&mut resp);
    resp
}

pub async fn serve_binary(
    state: &AppState,
    key: &str,
    exact: &Path,
    etag_source: &str,
    is_brotli: bool,
    method: &Method,
    headers: &HeaderMap,
) -> Response {
    let etag = format!("\"{etag_source}\"");

    if let Some(inm) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
        let matches = inm == "*"
            || inm
                .split(',')
                .map(|t| t.trim().trim_start_matches("W/"))
                .any(|t| t == etag);
        if matches {
            let mut resp = StatusCode::NOT_MODIFIED.into_response();
            let h = resp.headers_mut();
            h.insert("ETag", etag.parse().unwrap());
            h.insert(
                "Cache-Control",
                immutable_cache_control(is_brotli).parse().unwrap(),
            );
            apply_cors(&mut resp);
            return resp;
        }
    }

    let Some((path, size)) = resolve(state, key, exact).await else {
        return not_found();
    };

    let content_type = binary_content_type(etag_source);
    let base_headers = |resp: &mut Response| {
        let h = resp.headers_mut();
        h.insert("Content-Type", content_type.parse().unwrap());
        h.insert("ETag", etag.parse().unwrap());
        h.insert(
            "Cache-Control",
            immutable_cache_control(is_brotli).parse().unwrap(),
        );
        if is_brotli {
            h.insert("Content-Encoding", "br".parse().unwrap());
        } else {
            h.insert("Accept-Ranges", "bytes".parse().unwrap());
        }
    };

    if !is_brotli {
        let range_header = headers.get("range").and_then(|v| v.to_str().ok());
        match parse_range_header(range_header, Some(size)) {
            Some(ParsedRange::Unsatisfiable) => {
                let mut resp = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                resp.headers_mut()
                    .insert("Content-Range", format!("bytes */{size}").parse().unwrap());
                apply_cors(&mut resp);
                return resp;
            }
            Some(ParsedRange::Range { start, end }) => {
                let body = if *method == Method::HEAD {
                    Body::empty()
                } else {
                    match read_range(&path, start, end).await {
                        Some(b) => Body::from(b),
                        None => return not_found(),
                    }
                };
                let content_len = end - start + 1;
                let mut resp = (StatusCode::PARTIAL_CONTENT, body).into_response();
                base_headers(&mut resp);
                let h = resp.headers_mut();
                h.insert(
                    "Content-Range",
                    format!("bytes {start}-{end}/{size}").parse().unwrap(),
                );
                h.insert("Content-Length", content_len.to_string().parse().unwrap());
                apply_cors(&mut resp);
                return resp;
            }
            None => {}
        }
    }

    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        match stream_body(&path).await {
            Some(b) => b,
            None => return not_found(),
        }
    };
    let mut resp = (StatusCode::OK, body).into_response();
    base_headers(&mut resp);
    resp.headers_mut()
        .insert("Content-Length", size.to_string().parse().unwrap());
    apply_cors(&mut resp);
    resp
}
