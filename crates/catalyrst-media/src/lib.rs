pub mod backend;
pub mod cache;
pub mod config;
pub mod handlers;
pub mod http;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::backend::http::HttpBackend;
use crate::backend::mock::MockBackend;
use crate::backend::TranslationBackend;
use crate::config::{BackendKind, Config};

pub struct CachedConvert {
    pub at: Instant,
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub const CONVERT_CACHE_TTL: Duration = Duration::from_secs(300);

pub const CONVERT_CACHE_MAX_BODY: usize = 2 * 1024 * 1024;
const CONVERT_CACHE_MAX_ENTRIES: usize = 256;
const PINNED_CLIENT_MAX_ENTRIES: usize = 256;

pub struct AppStateInner {
    pub pool: PgPool,
    pub backend: Arc<dyn TranslationBackend>,
    pub backend_label: &'static str,
    pub fetch_client: reqwest::Client,

    pub pinned_clients: Mutex<HashMap<(String, SocketAddr), reqwest::Client>>,

    pub convert_cache: Mutex<HashMap<String, CachedConvert>>,
}

impl AppStateInner {
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

    pub fn convert_cache_get(&self, url: &str) -> Option<(u16, String, Vec<u8>)> {
        let c = self.convert_cache.lock().unwrap();
        let hit = c.get(url)?;
        if hit.at.elapsed() >= CONVERT_CACHE_TTL {
            return None;
        }
        Some((hit.status, hit.content_type.clone(), hit.body.clone()))
    }

    pub fn convert_cache_put(&self, url: &str, status: u16, content_type: &str, body: &[u8]) {
        if body.len() > CONVERT_CACHE_MAX_BODY {
            return;
        }
        let mut c = self.convert_cache.lock().unwrap();
        if c.len() >= CONVERT_CACHE_MAX_ENTRIES {
            c.clear();
        }
        c.insert(
            url.to_string(),
            CachedConvert {
                at: Instant::now(),
                status,
                content_type: content_type.to_string(),
                body: body.to_vec(),
            },
        );
    }
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid MEDIA_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect content pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let backend: Arc<dyn TranslationBackend> = match cfg.backend_kind {
        BackendKind::Mock => Arc::new(MockBackend),
        BackendKind::Http => Arc::new(HttpBackend::new(
            cfg.backend_url
                .clone()
                .expect("backend url checked in config"),
            cfg.backend_api_key.clone(),
        )),
    };

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
        pinned_clients: Mutex::new(HashMap::new()),
        convert_cache: Mutex::new(HashMap::new()),
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/translate", post(handlers::translate::translate))
        .route("/convert", get(handlers::convert::convert))
        .route("/media/convert", get(handlers::convert::convert))
}
