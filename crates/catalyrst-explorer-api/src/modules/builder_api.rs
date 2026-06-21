use crate::AppState;
use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/collections/{id}/items", get(get_collection_items))
        .route("/v1/storage/contents/{hash}", get(get_storage_content))
}

fn with_query(mut url: String, q: Option<String>) -> String {
    if let Some(q) = q.filter(|s| !s.is_empty()) {
        url.push('?');
        url.push_str(&q);
    }
    url
}

async fn get_collection_items(
    State(state): State<AppState>,
    Path(id): Path<String>,
    RawQuery(q): RawQuery,
) -> Response {
    let url = with_query(
        format!(
            "{}/v1/collections/{}/items",
            state.cfg.upstream_builder_url.trim_end_matches('/'),
            id
        ),
        q,
    );
    proxy(&state, &url).await
}

async fn get_storage_content(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    RawQuery(q): RawQuery,
) -> Response {
    let url = with_query(
        format!(
            "{}/v1/storage/contents/{}",
            state.cfg.upstream_builder_url.trim_end_matches('/'),
            hash
        ),
        q,
    );
    proxy(&state, &url).await
}

fn no_redirect_client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("catalyrst-explorer-api/0.1")
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build no-redirect reqwest client")
    })
}

async fn proxy(_state: &AppState, url: &str) -> Response {
    let resp = match no_redirect_client().get(url).send().await {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(%url, %err, "builder upstream request failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "upstream_unavailable" })),
            )
                .into_response();
        }
    };

    let status = resp.status();
    let mut headers = HeaderMap::new();
    for h in [
        header::CONTENT_TYPE,
        header::LOCATION,
        header::CACHE_CONTROL,
    ] {
        if let Some(v) = resp.headers().get(&h).cloned() {
            headers.insert(h, v);
        }
    }

    if status.is_redirection() {
        return (status, headers).into_response();
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
