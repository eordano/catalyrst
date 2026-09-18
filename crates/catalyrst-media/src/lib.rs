pub mod backend;
pub mod cache;
pub mod config;
pub mod handlers;
pub mod http;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::routing::{get, post};
use axum::Router;
use catalyrst_commons::http::resolve_and_pin;
use sqlx::PgPool;

use crate::backend::TranslationBackend;
use crate::config::Config;

pub struct CachedConvert {
    pub at: Instant,
    pub last_used: Instant,
    pub status: u16,
    pub content_type: String,
    pub body: Bytes,
}

pub const CONVERT_CACHE_TTL: Duration = Duration::from_secs(300);

pub const CONVERT_CACHE_MAX_BODY: usize = 2 * 1024 * 1024;
const CONVERT_CACHE_MAX_ENTRIES: usize = 256;
const PINNED_CLIENT_MAX_ENTRIES: usize = 256;
const PIN_TTL: Duration = Duration::from_secs(60);
const PIN_CACHE_MAX_ENTRIES: usize = 1024;

/// Per-URL fetch gate; the map entry goes away with the last holder.
pub struct ConvertGate<'a> {
    state: &'a AppStateInner,
    url: String,
    pub gate: Arc<tokio::sync::Mutex<()>>,
}

impl Drop for ConvertGate<'_> {
    fn drop(&mut self) {
        if let Ok(mut m) = self.state.convert_inflight.lock() {
            if Arc::strong_count(&self.gate) <= 2
                && m.get(&self.url).is_some_and(|g| Arc::ptr_eq(g, &self.gate))
            {
                m.remove(&self.url);
            }
        }
    }
}

pub struct AppStateInner {
    pub pool: PgPool,
    pub backend: Arc<dyn TranslationBackend>,
    pub backend_label: &'static str,
    pub fetch_client: reqwest::Client,

    pub translate_char_limit: usize,
    pub translate_batch_limit: usize,
    pub translate_timeout: Duration,

    pub pinned_clients: Mutex<HashMap<(String, SocketAddr), reqwest::Client>>,
    pub pins: Mutex<HashMap<String, (Instant, SocketAddr)>>,

    pub convert_cache: Mutex<HashMap<String, CachedConvert>>,
    pub convert_inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl AppStateInner {
    /// Only routable results are memoised, so an unroutable host is re-checked every time.
    pub async fn resolve_pinned(&self, url: &reqwest::Url) -> Option<SocketAddr> {
        let key = match (url.host_str(), url.port_or_known_default()) {
            (Some(host), Some(port)) => Some(format!("{host}:{port}")),
            _ => None,
        };
        if let Some(key) = &key {
            let hit = self.pins.lock().unwrap().get(key).copied();
            if let Some((at, addr)) = hit {
                if at.elapsed() < PIN_TTL {
                    return Some(addr);
                }
            }
        }
        let addr = resolve_and_pin(url).await?;
        if let Some(key) = key {
            let mut m = self.pins.lock().unwrap();
            if m.len() >= PIN_CACHE_MAX_ENTRIES && !m.contains_key(&key) {
                m.retain(|_, (at, _)| at.elapsed() < PIN_TTL);
                if m.len() >= PIN_CACHE_MAX_ENTRIES {
                    m.clear();
                }
            }
            m.insert(key, (Instant::now(), addr));
        }
        Some(addr)
    }

    pub fn convert_gate(&self, url: &str) -> ConvertGate<'_> {
        let gate = self
            .convert_inflight
            .lock()
            .ok()
            .map(|mut m| m.entry(url.to_string()).or_default().clone())
            .unwrap_or_default();
        ConvertGate {
            state: self,
            url: url.to_string(),
            gate,
        }
    }

    pub fn pinned_client(&self, host: &str, addr: SocketAddr) -> Result<reqwest::Client> {
        let key = (host.to_string(), addr);
        if let Some(c) = self.pinned_clients.lock().unwrap().get(&key) {
            return Ok(c.clone());
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent("catalyrst-media-converter/0.1")
            .redirect(reqwest::redirect::Policy::none())
            .resolve(host, addr)
            .build()
            .context("failed to build pinned fetch client")?;
        let mut m = self.pinned_clients.lock().unwrap();
        if m.len() >= PINNED_CLIENT_MAX_ENTRIES {
            m.clear();
        }
        m.insert(key, client.clone());
        Ok(client)
    }

    pub fn convert_cache_get(&self, url: &str) -> Option<(u16, String, Bytes)> {
        let mut c = self.convert_cache.lock().unwrap();
        let hit = c.get_mut(url)?;
        if hit.at.elapsed() >= CONVERT_CACHE_TTL {
            return None;
        }
        hit.last_used = Instant::now();
        Some((hit.status, hit.content_type.clone(), hit.body.clone()))
    }

    pub fn convert_cache_put(&self, url: &str, status: u16, content_type: &str, body: Bytes) {
        if body.len() > CONVERT_CACHE_MAX_BODY {
            return;
        }
        let mut c = self.convert_cache.lock().unwrap();
        if c.len() >= CONVERT_CACHE_MAX_ENTRIES && !c.contains_key(url) {
            c.retain(|_, v| v.at.elapsed() < CONVERT_CACHE_TTL);
            if c.len() >= CONVERT_CACHE_MAX_ENTRIES {
                if let Some(lru) = c
                    .iter()
                    .min_by_key(|(_, v)| v.last_used)
                    .map(|(k, _)| k.clone())
                {
                    c.remove(&lru);
                }
            }
        }
        let now = Instant::now();
        c.insert(
            url.to_string(),
            CachedConvert {
                at: now,
                last_used: now,
                status,
                content_type: content_type.to_string(),
                body,
            },
        );
    }
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool =
        catalyrst_db::connect_pool(&cfg.database_url, &catalyrst_db::PoolSettings::default())
            .await
            .context("failed to connect content pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let backend: Arc<dyn TranslationBackend> = backend::build_backend(cfg);

    let fetch_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent("catalyrst-media-converter/0.1")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build fetch client")?;

    Ok(Arc::new(AppStateInner {
        pool,
        backend,
        backend_label: cfg.backend_kind.label(),
        fetch_client,
        translate_char_limit: cfg.translate_char_limit,
        translate_batch_limit: cfg.translate_batch_limit,
        translate_timeout: cfg.translate_request_timeout,
        pinned_clients: Mutex::new(HashMap::new()),
        pins: Mutex::new(HashMap::new()),
        convert_cache: Mutex::new(HashMap::new()),
        convert_inflight: Mutex::new(HashMap::new()),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/translate", post(handlers::translate::translate))
        .route("/convert", get(handlers::convert::convert))
        .route("/media/convert", get(handlers::convert::convert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn convert_cache_evicts_least_recently_used_only() {
        let convert_cache = Mutex::new(HashMap::new());
        let pins = Mutex::new(HashMap::new());
        let inner = AppStateInner {
            pool: PgPool::connect_lazy("postgres://localhost/unused").unwrap(),
            backend: Arc::new(backend::mock::MockBackend),
            backend_label: "mock",
            fetch_client: reqwest::Client::new(),
            translate_char_limit: 1,
            translate_batch_limit: 1,
            translate_timeout: Duration::from_secs(1),
            pinned_clients: Mutex::new(HashMap::new()),
            pins,
            convert_cache,
            convert_inflight: Mutex::new(HashMap::new()),
        };
        for i in 0..CONVERT_CACHE_MAX_ENTRIES {
            inner.convert_cache_put(
                &format!("u{i}"),
                200,
                "text/plain",
                Bytes::from_static(b"x"),
            );
        }
        assert!(
            inner.convert_cache_get("u0").is_some(),
            "touch u0 so it is not the LRU"
        );
        inner.convert_cache_put("new", 200, "text/plain", Bytes::from_static(b"y"));
        assert_eq!(
            inner.convert_cache.lock().unwrap().len(),
            CONVERT_CACHE_MAX_ENTRIES
        );
        assert!(inner.convert_cache_get("u0").is_some());
        assert!(inner.convert_cache_get("u1").is_none());
        assert!(inner.convert_cache_get("new").is_some());

        let gate = inner.convert_gate("u9");
        assert_eq!(inner.convert_inflight.lock().unwrap().len(), 1);
        drop(gate);
        assert!(inner.convert_inflight.lock().unwrap().is_empty());
    }
}
