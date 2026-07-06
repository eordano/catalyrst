use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use bytes::Bytes;
use sqlx::postgres::PgListener;
use sqlx::PgPool;

use crate::ports::catalog_cache::DIRTY_CHANNEL;

const DEFAULT_TTL_SECS: u64 = 30;
const MAX_ENTRIES: usize = 512;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

struct Entry {
    generation: u64,
    at: Instant,
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

pub struct ResponseCache {
    enabled: bool,
    ttl: Duration,
    generation: AtomicU64,
    map: RwLock<HashMap<String, Entry>>,
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
            ttl: Duration::from_secs(ttl_secs.max(1)),
            generation: AtomicU64::new(0),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn cacheable_path(path: &str) -> bool {
        (path.starts_with("/v1/") || path.starts_with("/v2/") || path.starts_with("/federation/"))
            && !path.starts_with("/v1/admin")
    }

    fn lookup(&self, key: &str) -> Option<(StatusCode, HeaderMap, Bytes)> {
        let current = self.generation.load(Ordering::Relaxed);
        let map = self.map.read().ok()?;
        let e = map.get(key)?;
        if e.generation != current || e.at.elapsed() >= self.ttl {
            return None;
        }
        Some((e.status, e.headers.clone(), e.body.clone()))
    }

    fn store(&self, key: String, status: StatusCode, headers: HeaderMap, body: Bytes) {
        let generation = self.generation.load(Ordering::Relaxed);
        if let Ok(mut map) = self.map.write() {
            if map.len() >= MAX_ENTRIES {
                let ttl = self.ttl;
                map.retain(|_, e| e.generation == generation && e.at.elapsed() < ttl);
                if map.len() >= MAX_ENTRIES {
                    map.clear();
                }
            }
            map.insert(
                key,
                Entry {
                    generation,
                    at: Instant::now(),
                    status,
                    headers,
                    body,
                },
            );
        }
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
    tokio::spawn(async move {
        loop {
            match PgListener::connect_with(&pool).await {
                Ok(mut listener) => {
                    cache.bump_generation();
                    if let Err(err) = listener.listen(DIRTY_CHANNEL).await {
                        tracing::warn!(%err, "http cache LISTEN failed; retrying");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                    tracing::info!(channel = DIRTY_CHANNEL, "http response cache listener up");
                    loop {
                        match listener.recv().await {
                            Ok(_) => cache.bump_generation(),
                            Err(err) => {
                                tracing::warn!(%err, "http cache listener dropped; reconnecting");
                                cache.bump_generation();
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(%err, "http cache listener connect failed; retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
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
        assert!(!ResponseCache::cacheable_path("/v1/admin/audit"));
        assert!(!ResponseCache::cacheable_path("/ping"));
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
