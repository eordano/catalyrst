use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ContentParams {
    pub ts: Option<String>,
}

fn content_url(state: &AppState, hash: &str, params: &ContentParams) -> String {
    let bucket = state.content_bucket_url.trim_end_matches('/');
    let mut target = format!("{}/contents/{}", bucket, hash);
    if let Some(ts) = params.ts.as_deref().filter(|s| !s.is_empty()) {
        target.push_str("?ts=");
        target.push_str(ts);
    }
    target
}

pub async fn get_storage_content(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    let target = content_url(&state, &hash, &params);

    let mut resp = (StatusCode::MOVED_PERMANENTLY, ()).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&target) {
        headers.insert(header::LOCATION, v);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public,max-age=31536000,immutable"),
    );
    resp
}

/// Existence cache for HEAD /v1/storage/contents/{hash}/exists. Content hashes
/// are immutable, so a positive result is valid indefinitely; entries still
/// expire so a not-yet-uploaded hash that appears later isn't pinned as missing.
/// Without this, every probe paid a full remote HEAD round-trip to the content
/// bucket (~330ms p50). The lock is never held across the await.
const EXISTS_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn exists_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>> {
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>>,
    > = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub async fn head_storage_content_exists(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    let target = content_url(&state, &hash, &params);

    if let Some((at, ok)) = exists_cache()
        .lock()
        .ok()
        .and_then(|m| m.get(&target).copied())
    {
        if at.elapsed() < EXISTS_TTL {
            return if ok {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
            .into_response();
        }
    }

    let ok = matches!(state.http.head(&target).send().await, Ok(r) if r.status().is_success());
    if let Ok(mut m) = exists_cache().lock() {
        m.insert(target, (std::time::Instant::now(), ok));
    }
    if ok {
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
