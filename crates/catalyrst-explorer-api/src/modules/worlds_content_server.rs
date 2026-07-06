use crate::AppState;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/world/{name}/about", get(world_about))
        .route("/world/{name}/permissions", get(world_permissions))
        .route("/worlds/{name}/comms", post(worlds_comms))
        .route(
            "/worlds/{name}/scenes/{scene_id}/comms",
            post(worlds_scene_comms),
        )
        .route(
            "/contents/{hash}",
            get(worlds_contents).head(worlds_contents_head),
        )
        .route("/wallet/{wallet}/connected-world", get(connected_world))
}

async fn world_about(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let url = format!(
        "{}/world/{}/about",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&name),
    );
    proxy_get(&state, &url, None).await
}

async fn world_permissions(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let url = format!(
        "{}/world/{}/permissions",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&name),
    );
    proxy_get(&state, &url, None).await
}

async fn worlds_comms(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let url = format!(
        "{}/worlds/{}/comms",
        state.cfg.upstream_worlds_url.trim_end_matches('/'),
        urlencoding::encode(&name),
    );
    proxy(&state, Method::POST, &url, Some(&headers), Some(body)).await
}

async fn worlds_scene_comms(
    State(state): State<AppState>,
    Path((name, scene_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let url = format!(
        "{}/worlds/{}/scenes/{}/comms",
        state.cfg.upstream_worlds_url.trim_end_matches('/'),
        urlencoding::encode(&name),
        urlencoding::encode(&scene_id),
    );
    proxy(&state, Method::POST, &url, Some(&headers), Some(body)).await
}

async fn worlds_contents(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    let url = format!(
        "{}/contents/{}",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&hash),
    );
    proxy_get(&state, &url, Some(&headers)).await
}

async fn worlds_contents_head(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    let url = format!(
        "{}/contents/{}",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&hash),
    );
    proxy(&state, Method::HEAD, &url, Some(&headers), None).await
}

async fn connected_world(State(state): State<AppState>, Path(wallet): Path<String>) -> Response {
    let url = format!(
        "{}/wallet/{}/connected-world",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&wallet),
    );
    proxy_get(&state, &url, None).await
}

async fn proxy_get(state: &AppState, url: &str, forward_headers: Option<&HeaderMap>) -> Response {
    proxy(state, Method::GET, url, forward_headers, None).await
}

async fn proxy(
    state: &AppState,
    method: Method,
    url: &str,
    forward_headers: Option<&HeaderMap>,
    body: Option<Bytes>,
) -> Response {
    let mut req = state.http.request(method, url);
    if let Some(h) = forward_headers {
        for (name, value) in h.iter() {
            if should_forward(name) {
                req = req.header(name.as_str(), value);
            }
        }
    }
    if let Some(b) = body {
        req = req.body(b);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(%url, %err, "worlds upstream request failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "upstream_unavailable" })),
            )
                .into_response();
        }
    };

    proxy_response(resp).await
}

async fn proxy_response(resp: reqwest::Response) -> Response {
    let status = resp.status();
    let mut headers = HeaderMap::new();
    for h in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::CACHE_CONTROL,
        header::ETAG,
    ] {
        if let Some(v) = resp.headers().get(&h).cloned() {
            headers.insert(h, v);
        }
    }
    if !headers.contains_key(header::CONTENT_TYPE) {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
    }
    let body = Body::from_stream(resp.bytes_stream());
    (status, headers, body).into_response()
}

fn should_forward(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "range" | "authorization" | "if-none-match" | "if-modified-since"
    ) || name.as_str().starts_with("x-identity-")
        || name.as_str().starts_with("x-signature-")
        || name.as_str().starts_with("x-timestamp")
        || name.as_str().starts_with("dcl-")
}
