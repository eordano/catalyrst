use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use sqlx::PgPool;

use super::catalog::{CatalogFilters, CatalogItem};

pub const DIRTY_CHANNEL: &str = "catalyrst_market_dirty";
const DEFAULT_TTL_SECS: u64 = 30;
const MAX_ENTRIES: usize = 256;

type Key = (bool, CatalogFilters);
type Page = Arc<(Vec<CatalogItem>, i64)>;

pub struct CatalogCache {
    enabled: bool,
    entries: TtlMap<Key, Page>,
}

impl CatalogCache {
    pub fn from_env() -> Self {
        let ttl_secs = std::env::var("CATALYRST_MARKET_CATALOG_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TTL_SECS);
        Self::new(ttl_secs)
    }

    pub fn new(ttl_secs: u64) -> Self {
        Self {
            enabled: ttl_secs > 0,
            entries: TtlMap::new("market.catalog_pages", Duration::from_secs(ttl_secs.max(1))),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn bump_generation(&self) {
        self.entries.bump_generation();
    }

    pub fn lookup(&self, key: &Key) -> Option<Page> {
        if !self.enabled {
            return None;
        }
        self.entries.get_fresh(key)
    }

    pub fn store(&self, key: Key, page: Page) {
        if !self.enabled {
            return;
        }
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.retain_fresh();
            if self.entries.len() >= MAX_ENTRIES {
                self.entries.clear();
            }
        }
        self.entries.insert(key, page);
    }
}

pub fn spawn_invalidation_listener(pool: PgPool, cache: Arc<CatalogCache>) {
    if !cache.enabled() {
        tracing::info!("catalog cache disabled (CATALYRST_MARKET_CATALOG_CACHE_TTL_SECS=0)");
        return;
    }
    catalyrst_commons::worker::spawn_invalidation_listener(pool, DIRTY_CHANNEL, move || {
        cache.bump_generation()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(first: Option<i64>) -> Key {
        (
            false,
            CatalogFilters {
                first,
                ..Default::default()
            },
        )
    }

    fn page() -> Page {
        Arc::new((Vec::new(), 42))
    }

    #[test]
    fn hit_within_same_generation_and_ttl() {
        let c = CatalogCache::new(60);
        c.store(key(Some(24)), page());
        assert!(c.lookup(&key(Some(24))).is_some());
        assert!(
            c.lookup(&key(Some(48))).is_none(),
            "different key must miss"
        );
    }

    #[test]
    fn generation_bump_invalidates_everything() {
        let c = CatalogCache::new(60);
        c.store(key(Some(24)), page());
        c.bump_generation();
        assert!(
            c.lookup(&key(Some(24))).is_none(),
            "a NOTIFY (generation bump) must invalidate cached pages"
        );
    }

    #[test]
    fn ttl_zero_disables() {
        let c = CatalogCache::new(0);
        c.store(key(Some(24)), page());
        assert!(!c.enabled());
        assert!(c.lookup(&key(Some(24))).is_none());
    }

    #[test]
    fn store_after_bump_serves_new_generation() {
        let c = CatalogCache::new(60);
        c.store(key(Some(24)), page());
        c.bump_generation();
        c.store(key(Some(24)), page());
        assert!(c.lookup(&key(Some(24))).is_some());
    }

    #[test]
    fn cap_bounds_entries() {
        let c = CatalogCache::new(60);
        for i in 0..(MAX_ENTRIES as i64 + 40) {
            c.store(key(Some(i)), page());
        }
        let len = c.entries.len();
        assert!(len <= MAX_ENTRIES + 1, "map grew past cap: {len}");
    }
}
