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
    let headers = resp.headers().clone();
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
}
