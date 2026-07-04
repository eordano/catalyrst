use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use sqlx::postgres::PgListener;
use sqlx::PgPool;

use super::catalog::{CatalogFilters, CatalogItem};

pub const DIRTY_CHANNEL: &str = "catalyrst_market_dirty";
const DEFAULT_TTL_SECS: u64 = 30;
const MAX_ENTRIES: usize = 256;

type Key = (bool, CatalogFilters);
type Page = Arc<(Vec<CatalogItem>, i64)>;

struct Entry {
    generation: u64,
    at: Instant,
    page: Page,
}

pub struct CatalogCache {
    enabled: bool,
    ttl: Duration,
    generation: AtomicU64,
    map: RwLock<HashMap<Key, Entry>>,
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
            ttl: Duration::from_secs(ttl_secs.max(1)),
            generation: AtomicU64::new(0),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn lookup(&self, key: &Key) -> Option<Page> {
        if !self.enabled {
            return None;
        }
        let current = self.generation.load(Ordering::Relaxed);
        let map = self.map.read().ok()?;
        let entry = map.get(key)?;
        if entry.generation != current || entry.at.elapsed() >= self.ttl {
            return None;
        }
        Some(Arc::clone(&entry.page))
    }

    pub fn store(&self, key: Key, page: Page) {
        if !self.enabled {
            return;
        }
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
                    page,
                },
            );
        }
    }
}

pub fn spawn_invalidation_listener(pool: PgPool, cache: Arc<CatalogCache>) {
    if !cache.enabled() {
        tracing::info!("catalog cache disabled (CATALYRST_MARKET_CATALOG_CACHE_TTL_SECS=0)");
        return;
    }
    tokio::spawn(async move {
        loop {
            match PgListener::connect_with(&pool).await {
                Ok(mut listener) => {
                    cache.bump_generation();
                    if let Err(err) = listener.listen(DIRTY_CHANNEL).await {
                        tracing::warn!(%err, "catalog cache LISTEN failed; retrying");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                    tracing::info!(
                        channel = DIRTY_CHANNEL,
                        "catalog cache invalidation listener up"
                    );
                    loop {
                        match listener.recv().await {
                            Ok(_notification) => cache.bump_generation(),
                            Err(err) => {
                                tracing::warn!(%err, "catalog cache listener dropped; reconnecting");
                                cache.bump_generation();
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(%err, "catalog cache listener connect failed; retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
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
        let len = c.map.read().unwrap().len();
        assert!(len <= MAX_ENTRIES + 1, "map grew past cap: {len}");
    }
}
