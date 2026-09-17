use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use bytes::Bytes;
use catalyrst_commons::cache::TtlMap;
use sqlx::PgPool;

use crate::auth_chain::AUTH_CHAIN_HEADER_PREFIX;
use crate::ports::catalog_cache::DIRTY_CHANNEL;

const DEFAULT_TTL_SECS: u64 = 30;
const MAX_ENTRIES: usize = 512;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
struct Entry {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

pub struct ResponseCache {
    enabled: bool,
    entries: TtlMap<String, Entry>,
}

impl ResponseCache {
    pub fn from_env() -> Arc<Self> {
        let ttl_secs = std::env::var("CATALYRST_MARKET_HTTP_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TTL_SECS);
        Arc::new(Self::new(ttl_secs))
    }

    pub fn new(ttl_secs: u64) -> Self {
        Self {
            enabled: ttl_secs > 0,
            entries: TtlMap::new(
                "market.http_responses",
                Duration::from_secs(ttl_secs.max(1)),
            ),
        }
    }

    pub fn bump_generation(&self) {
        self.entries.bump_generation();
    }

    fn cacheable_path(path: &str) -> bool {
        const EXACT: &[&str] = &[
            "/v1/contracts",
            "/v1/collections",
            "/v1/accounts",
            "/v1/owners",
            "/v1/catalog",
            "/v2/catalog",
            "/v1/nfts",
            "/v1/items",
            "/v1/orders",
            "/v1/bids",
            "/v1/sales",
            "/v1/prices",
            "/v1/trendings",
            "/v1/trades",
            "/v1/federation/bids",
            "/v1/federation/orders",
            "/v1/federation/trades",
            "/federation/market/snapshot",
            "/federation/market/changes",
        ];
        const PREFIXES: &[&str] = &[
            "/v1/users/",
            "/v1/rankings/",
            "/v1/stats/",
            "/v1/volume/",
            "/v1/trades/",
        ];
        EXACT.contains(&path) || PREFIXES.iter().any(|p| path.starts_with(p))
    }

    /// A credentialed request can carry per-caller fields in its body (`/v1/catalog`'s
    /// `picks[].pickedByUser`), and the key is path+query only -- so a credentialed response
    /// must never enter or be served from this shared store. Any header that identifies a
    /// caller counts, not just the signed-fetch chain. Keep this guard on any new caller.
    fn carries_credentials(headers: &HeaderMap) -> bool {
        headers.keys().any(|k| {
            let name = k.as_str();
            name.starts_with(AUTH_CHAIN_HEADER_PREFIX)
                || name == "authorization"
                || name == "proxy-authorization"
                || name == "cookie"
        })
    }

    /// Only the headers that describe the body itself are replayed. A stored `set-cookie`,
    /// `date` or per-request trace header would be handed to every later caller of the same
    /// path, which is how a shared cache leaks one caller's session into another's response.
    fn cacheable_headers(headers: &HeaderMap) -> HeaderMap {
        const KEEP: &[&str] = &[
            "content-type",
            "content-encoding",
            "content-language",
            "cache-control",
            "etag",
            "last-modified",
            "vary",
        ];
        let mut out = HeaderMap::new();
        for name in KEEP {
            for value in headers.get_all(*name) {
                if let Ok(header) = axum::http::HeaderName::try_from(*name) {
                    out.append(header, value.clone());
                }
            }
        }
        out
    }

    /// A response that names itself `private` or `no-store`, or whose `Vary` makes the answer
    /// depend on a request header the key does not carry, is not the same answer for the next
    /// caller. The key is path+query only, so such a response never enters the shared store.
    fn is_shareable(headers: &HeaderMap) -> bool {
        for value in headers.get_all(axum::http::header::CACHE_CONTROL) {
            let Ok(text) = value.to_str() else {
                return false;
            };
            for directive in text.split(',') {
                let token = directive.split('=').next().unwrap_or(directive).trim();
                if token.eq_ignore_ascii_case("no-store") || token.eq_ignore_ascii_case("private") {
                    return false;
                }
            }
        }
        for value in headers.get_all(axum::http::header::VARY) {
            let Ok(text) = value.to_str() else {
                return false;
            };
            for name in text.split(',') {
                let name = name.trim();
                if !name.is_empty() && !name.eq_ignore_ascii_case("accept-encoding") {
                    return false;
                }
            }
        }
        true
    }

    fn lookup(&self, key: &str) -> Option<(StatusCode, HeaderMap, Bytes)> {
        let e = self.entries.get_fresh(&key.to_string())?;
        Some((e.status, e.headers, e.body))
    }

    fn store(&self, key: String, status: StatusCode, headers: HeaderMap, body: Bytes) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.retain_fresh();
            if self.entries.len() >= MAX_ENTRIES {
                self.entries.clear();
            }
        }
        self.entries.insert(
            key,
            Entry {
                status,
                headers,
                body,
            },
        );
    }
}

pub async fn middleware(
    State(cache): State<Arc<ResponseCache>>,
    req: Request,
    next: Next,
) -> Response {
    if !cache.enabled
        || req.method() != Method::GET
        || !ResponseCache::cacheable_path(req.uri().path())
        || ResponseCache::carries_credentials(req.headers())
    {
        return next.run(req).await;
    }

    let key = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    if let Some((status, headers, body)) = cache.lookup(&key) {
        let mut resp = Response::new(Body::from(body));
        *resp.status_mut() = status;
        *resp.headers_mut() = headers;
        return resp;
    }

    let resp = next.run(req).await;
    let status = resp.status();
    if status != StatusCode::OK {
        return resp;
    }
    if !ResponseCache::is_shareable(resp.headers()) {
        return resp;
    }
    let headers = ResponseCache::cacheable_headers(resp.headers());
    let (parts, body) = resp.into_parts();
    match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(bytes) => {
            cache.store(key, status, headers, bytes.clone());
            Response::from_parts(parts, Body::from(bytes))
        }
        Err(_) => {
            let mut resp = Response::new(Body::empty());
            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            resp
        }
    }
}

pub fn spawn_invalidation_listener(pool: PgPool, cache: Arc<ResponseCache>) {
    if !cache.enabled {
        tracing::info!("http response cache disabled (CATALYRST_MARKET_HTTP_CACHE_TTL_SECS=0)");
        return;
    }
    catalyrst_commons::worker::spawn_invalidation_listener(pool, DIRTY_CHANNEL, move || {
        cache.bump_generation()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cacheable_paths() {
        assert!(ResponseCache::cacheable_path("/v1/nfts"));
        assert!(ResponseCache::cacheable_path("/v2/catalog"));
        assert!(ResponseCache::cacheable_path("/federation/market/snapshot"));
        assert!(ResponseCache::cacheable_path("/v1/users/0xabc/wearables"));
        assert!(!ResponseCache::cacheable_path("/v1/admin/audit"));
        assert!(!ResponseCache::cacheable_path("/ping"));
    }

    #[test]
    fn auth_bearing_paths_never_cached() {
        assert!(!ResponseCache::cacheable_path("/v1/lists"));
        assert!(!ResponseCache::cacheable_path("/v1/activity"));
        assert!(!ResponseCache::cacheable_path("/v1/picks/0xdead-1"));
        assert!(
            !ResponseCache::cacheable_path("/v1/picks/stats"),
            "the bulk pick stats vary with checkingUserAddress"
        );
    }

    fn headers_with(name: &'static str, value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
        headers
    }

    #[test]
    fn a_signed_request_bypasses_the_shared_store() {
        assert!(
            ResponseCache::carries_credentials(&headers_with("x-identity-auth-chain-0", "{}")),
            "a signed /v1/catalog embeds that signer's pickedByUser flags"
        );
        assert!(!ResponseCache::carries_credentials(&headers_with(
            "x-anonymous-id",
            "abc"
        )));
        assert!(!ResponseCache::carries_credentials(&HeaderMap::new()));
    }

    #[test]
    fn any_credential_header_bypasses_the_shared_store() {
        for name in ["authorization", "proxy-authorization", "cookie"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::HeaderName::try_from(name).unwrap(),
                axum::http::HeaderValue::from_static("secret"),
            );
            assert!(
                ResponseCache::carries_credentials(&headers),
                "{name} identifies a caller, so its response is not shareable"
            );
        }
    }

    #[test]
    fn a_stored_response_replays_only_headers_that_describe_its_body() {
        let mut upstream = HeaderMap::new();
        upstream.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        );
        upstream.insert(
            axum::http::header::VARY,
            axum::http::HeaderValue::from_static("origin"),
        );
        upstream.append(
            axum::http::header::SET_COOKIE,
            axum::http::HeaderValue::from_static("session=abc"),
        );
        upstream.insert(
            axum::http::HeaderName::from_static("x-request-id"),
            axum::http::HeaderValue::from_static("req-1"),
        );
        let kept = ResponseCache::cacheable_headers(&upstream);
        assert_eq!(
            kept.get(axum::http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(kept.get(axum::http::header::VARY).unwrap(), "origin");
        assert!(
            kept.get(axum::http::header::SET_COOKIE).is_none(),
            "a replayed Set-Cookie hands one caller's session to the next"
        );
        assert!(kept.get("x-request-id").is_none());
    }

    #[test]
    fn generation_and_ttl_protocol() {
        let c = ResponseCache::new(60);
        c.store(
            "/v1/nfts?first=24".into(),
            StatusCode::OK,
            HeaderMap::new(),
            Bytes::from_static(b"{}"),
        );
        assert!(c.lookup("/v1/nfts?first=24").is_some());
        assert!(c.lookup("/v1/nfts?first=48").is_none());
        c.bump_generation();
        assert!(
            c.lookup("/v1/nfts?first=24").is_none(),
            "NOTIFY bump must invalidate"
        );
    }

    #[test]
    fn ttl_zero_disables() {
        let c = ResponseCache::new(0);
        assert!(!c.enabled);
    }

    async fn handler_hits_for(response_headers: &[(&'static str, &'static str)]) -> usize {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tower::ServiceExt;

        let hits = Arc::new(AtomicUsize::new(0));
        let handler_hits = hits.clone();
        let emitted = response_headers.to_vec();
        let app = axum::Router::new()
            .route(
                "/v1/nfts",
                axum::routing::get(move || {
                    let hits = handler_hits.clone();
                    let emitted = emitted.clone();
                    async move {
                        hits.fetch_add(1, Ordering::SeqCst);
                        let mut resp = Response::new(Body::from("{}"));
                        for (name, value) in emitted {
                            resp.headers_mut().append(
                                axum::http::HeaderName::from_static(name),
                                axum::http::HeaderValue::from_static(value),
                            );
                        }
                        resp
                    }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                Arc::new(ResponseCache::new(60)),
                middleware,
            ));

        for _ in 0..2 {
            let req = Request::builder()
                .uri("/v1/nfts?first=24")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        hits.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn a_response_that_calls_itself_private_is_not_served_to_a_second_caller() {
        assert_eq!(
            handler_hits_for(&[]).await,
            1,
            "an unmarked response is the same answer for everyone, so the second caller is served \
             the stored one"
        );
        assert_eq!(
            handler_hits_for(&[("cache-control", "private, max-age=30")]).await,
            2
        );
        assert_eq!(handler_hits_for(&[("cache-control", "no-store")]).await, 2);
    }

    #[tokio::test]
    async fn a_response_that_varies_by_a_request_header_is_not_served_to_a_second_caller() {
        assert_eq!(
            handler_hits_for(&[("vary", "Authorization")]).await,
            2,
            "the key is path+query only, so a Vary the key does not carry cannot be honoured"
        );
        assert_eq!(
            handler_hits_for(&[("vary", "Origin, Accept-Encoding")]).await,
            2
        );
        assert_eq!(
            handler_hits_for(&[("vary", "Accept-Encoding")]).await,
            1,
            "content coding is already negotiated per stored body"
        );
    }
}
