use crate::AppState;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use catalyrst_commons::cache::TtlMap;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const WORLD_DOC_TTL: Duration = Duration::from_secs(10);
pub(crate) const CONTENTS_TTL: Duration = Duration::from_secs(600);
pub(crate) const CONTENTS_MAX_ENTRIES: usize = 256;
const WORLD_DOC_MAX_BYTES: u64 = 1 << 20;
const CONTENTS_MAX_BYTES: u64 = 256 << 10;
const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");

/// A buffered upstream 200: the forwarded headers plus the whole body.
pub(crate) struct CachedUpstream {
    headers: HeaderMap,
    body: Bytes,
}

impl CachedUpstream {
    fn response(&self, with_body: bool) -> Response {
        let body = if with_body {
            Body::from(self.body.clone())
        } else {
            Body::empty()
        };
        (StatusCode::OK, self.headers.clone(), body).into_response()
    }
}

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
    proxy_memo(
        &state,
        &state.world_doc_cache,
        format!("about:{name}"),
        &url,
    )
    .await
}

async fn world_permissions(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let url = format!(
        "{}/world/{}/permissions",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&name),
    );
    proxy_memo(&state, &state.world_doc_cache, format!("perm:{name}"), &url).await
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
    proxy(
        &state,
        Method::POST,
        &url,
        Some(&headers),
        Some(body),
        false,
    )
    .await
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
    proxy(
        &state,
        Method::POST,
        &url,
        Some(&headers),
        Some(body),
        false,
    )
    .await
}

fn contents_url(state: &AppState, hash: &str) -> String {
    format!(
        "{}/contents/{}",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(hash),
    )
}

/// Range, conditional and authorized fetches bypass the hash cache so their upstream
/// semantics (206, 304, per-caller answers) are untouched.
fn bypasses_contents_cache(headers: &HeaderMap) -> bool {
    [
        header::RANGE,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
        header::AUTHORIZATION,
    ]
    .iter()
    .any(|h| headers.contains_key(h))
}

async fn worlds_contents(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    let cacheable = !bypasses_contents_cache(&headers);
    if cacheable {
        if let Some(doc) = state.contents_cache.get_fresh(&hash) {
            return doc.response(true);
        }
    }
    let url = contents_url(&state, &hash);
    let resp = match send(&state, Method::GET, &url, Some(&headers), None).await {
        Ok(resp) => resp,
        Err(failed) => return failed,
    };
    if !cacheable {
        return proxy_response(resp, true).await;
    }
    match buffer_cacheable(resp, CONTENTS_MAX_BYTES, true).await {
        Ok(doc) => {
            state.contents_cache.insert(hash, doc.clone());
            doc.response(true)
        }
        Err(passthrough) => passthrough,
    }
}

async fn worlds_contents_head(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !bypasses_contents_cache(&headers) {
        if let Some(doc) = state.contents_cache.get_fresh(&hash) {
            return doc.response(false);
        }
    }
    let url = contents_url(&state, &hash);
    proxy(&state, Method::HEAD, &url, Some(&headers), None, true).await
}

async fn connected_world(State(state): State<AppState>, Path(wallet): Path<String>) -> Response {
    let url = format!(
        "{}/wallet/{}/connected-world",
        state.cfg.upstream_worlds_content_url.trim_end_matches('/'),
        urlencoding::encode(&wallet),
    );
    proxy(&state, Method::GET, &url, None, None, false).await
}

async fn proxy_memo(
    state: &AppState,
    cache: &TtlMap<String, Arc<CachedUpstream>>,
    key: String,
    url: &str,
) -> Response {
    if let Some(doc) = cache.get_fresh(&key) {
        return doc.response(true);
    }
    let resp = match send(state, Method::GET, url, None, None).await {
        Ok(resp) => resp,
        Err(failed) => return failed,
    };
    match buffer_cacheable(resp, WORLD_DOC_MAX_BYTES, false).await {
        Ok(doc) => {
            cache.insert(key, doc.clone());
            doc.response(true)
        }
        Err(passthrough) => passthrough,
    }
}

async fn proxy(
    state: &AppState,
    method: Method,
    url: &str,
    forward_headers: Option<&HeaderMap>,
    body: Option<Bytes>,
    immutable: bool,
) -> Response {
    match send(state, method, url, forward_headers, body).await {
        Ok(resp) => proxy_response(resp, immutable).await,
        Err(failed) => failed,
    }
}

async fn send(
    state: &AppState,
    method: Method,
    url: &str,
    forward_headers: Option<&HeaderMap>,
    body: Option<Bytes>,
) -> Result<reqwest::Response, Response> {
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
    req.send().await.map_err(|err| {
        tracing::warn!(%url, %err, "worlds upstream request failed");
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "upstream_unavailable" })),
        )
            .into_response()
    })
}

/// Buffers a 200 whose advertised length fits the cap; anything else streams through
/// untouched (the `Err` is the ready-to-send passthrough response).
async fn buffer_cacheable(
    resp: reqwest::Response,
    max_bytes: u64,
    immutable: bool,
) -> Result<Arc<CachedUpstream>, Response> {
    let fits = resp.status() == StatusCode::OK
        && resp.content_length().is_some_and(|len| len <= max_bytes);
    if !fits {
        return Err(proxy_response(resp, immutable).await);
    }
    let mut headers = forwarded_headers(&resp, immutable);
    let body = match resp.bytes().await {
        Ok(body) => body,
        Err(err) => {
            tracing::warn!(%err, "worlds upstream body read failed");
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "upstream_unavailable" })),
            )
                .into_response());
        }
    };
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
    Ok(Arc::new(CachedUpstream { headers, body }))
}

fn forwarded_headers(resp: &reqwest::Response, immutable: bool) -> HeaderMap {
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
    let status = resp.status();
    if immutable
        && (status == StatusCode::OK || status == StatusCode::PARTIAL_CONTENT)
        && !headers.contains_key(header::CACHE_CONTROL)
    {
        headers.insert(header::CACHE_CONTROL, IMMUTABLE);
    }
    headers
}

async fn proxy_response(resp: reqwest::Response, immutable: bool) -> Response {
    let status = resp.status();
    let headers = forwarded_headers(&resp, immutable);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::IntoFuture;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use tower::ServiceExt;

    async fn upstream(hits: Arc<AtomicUsize>) -> std::net::SocketAddr {
        let counted = move |body: &'static str, status: StatusCode| {
            let hits = hits.clone();
            move || {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, SeqCst);
                    (status, [(header::CONTENT_TYPE, "application/json")], body)
                }
            }
        };
        let app = axum::Router::new()
            .route(
                "/world/{name}/about",
                get(counted(r#"{"healthy":true}"#, StatusCode::OK)),
            )
            .route(
                "/world/{name}/permissions",
                get(counted(r#"{"deployment":{}}"#, StatusCode::NOT_FOUND)),
            )
            .route(
                "/contents/{hash}",
                get(counted("blob", StatusCode::OK)).head(counted("", StatusCode::OK)),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(axum::serve(l, app).into_future());
        addr
    }

    async fn app_for(addr: std::net::SocketAddr) -> axum::Router {
        let mut cfg = crate::config::Config::from_env().unwrap();
        cfg.upstream_worlds_content_url = format!("http://{addr}");
        let state = crate::build_state(&cfg).await.unwrap();
        routes().with_state(state)
    }

    async fn call(
        app: &axum::Router,
        method: &str,
        path: &str,
        extra: &[(&str, &str)],
    ) -> Response {
        let mut req = axum::http::Request::builder().method(method).uri(path);
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        app.clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn world_about_is_memoized_but_upstream_errors_are_not() {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = app_for(upstream(hits.clone()).await).await;

        for _ in 0..5 {
            let resp = call(&app, "GET", "/world/foo.dcl.eth/about", &[]).await;
            assert_eq!(resp.status(), StatusCode::OK);
            let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .unwrap();
            assert_eq!(&body[..], br#"{"healthy":true}"#);
        }
        assert_eq!(
            hits.load(SeqCst),
            1,
            "about doc must be served from the memo"
        );

        for _ in 0..3 {
            let resp = call(&app, "GET", "/world/foo.dcl.eth/permissions", &[]).await;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(hits.load(SeqCst), 4, "a 404 must never be memoized");
    }

    #[tokio::test]
    async fn contents_are_cached_by_hash_with_immutable_cache_control() {
        let hits = Arc::new(AtomicUsize::new(0));
        let app = app_for(upstream(hits.clone()).await).await;

        for _ in 0..3 {
            let resp = call(&app, "GET", "/contents/bafyhash", &[]).await;
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(
                resp.headers().get(header::CACHE_CONTROL).unwrap(),
                "public, max-age=31536000, immutable"
            );
            assert_eq!(resp.headers().get(header::CONTENT_LENGTH).unwrap(), "4");
            let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .unwrap();
            assert_eq!(&body[..], b"blob");
        }
        assert_eq!(hits.load(SeqCst), 1);

        let resp = call(&app, "HEAD", "/contents/bafyhash", &[]).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(header::CONTENT_LENGTH).unwrap(), "4");
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(body.is_empty());
        assert_eq!(hits.load(SeqCst), 1, "HEAD must answer from the hash cache");

        let resp = call(&app, "GET", "/contents/bafyhash", &[("range", "bytes=0-1")]).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            hits.load(SeqCst),
            2,
            "a range request must bypass the cache"
        );
    }
}
