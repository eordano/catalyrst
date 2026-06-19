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

pub async fn head_storage_content_exists(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    let target = content_url(&state, &hash, &params);

    match state.http.head(&target).send().await {
        Ok(r) if r.status().is_success() => StatusCode::OK.into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
