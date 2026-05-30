use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

pub type ResolveCache = Cache<String, Option<(PathBuf, u64)>>;

pub struct AppStateInner {
    pub out_root: PathBuf,
    pub resolve_cache: ResolveCache,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn new(out_root: PathBuf) -> Self {
        Self {
            out_root,
            resolve_cache: Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
        }
    }
}
