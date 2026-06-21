use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

pub type ResolveCache = Cache<String, Option<(PathBuf, u64)>>;

pub struct AppStateInner {
    pub out_root: PathBuf,
    pub resolve_cache: ResolveCache,
    /// Live-conversion upstream (abgen-serve). When set, local misses proxy here.
    pub live_upstream: Option<String>,
    /// Shared client for the upstream proxy. No overall request timeout: a JIT
    /// bundle build can take many seconds and abgen-serve owns its own
    /// build/fallback deadline, so we only bound the connect.
    pub http: reqwest::Client,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn new(out_root: PathBuf, live_upstream: Option<String>) -> Self {
        Self {
            out_root,
            resolve_cache: Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            live_upstream,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }
}
