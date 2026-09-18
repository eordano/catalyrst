use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use sqlx::PgPool;

use super::catalog::{CatalogFilters, CatalogItem, PickStats};

pub const DIRTY_CHANNEL: &str = "catalyrst_market_dirty";
const DEFAULT_TTL_SECS: u64 = 30;
const MAX_ENTRIES: usize = 256;

type Key = (bool, CatalogFilters);
type Page = Arc<(Vec<CatalogItem>, i64)>;
type Picks = Arc<Vec<PickStats>>;

pub struct CatalogCache {
    enabled: bool,
    entries: TtlMap<Key, Page>,
    totals: TtlMap<CatalogFilters, i64>,
    picks: TtlMap<Vec<String>, Picks>,
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
            entries: TtlMap::bounded(
                "market.catalog_pages",
                Duration::from_secs(ttl_secs.max(1)),
                MAX_ENTRIES,
            ),
            totals: TtlMap::bounded(
                "market.catalog_totals",
                Duration::from_secs(ttl_secs.max(1)),
                MAX_ENTRIES,
            ),
            picks: TtlMap::bounded(
                "market.catalog_picks",
                Duration::from_secs(ttl_secs.max(1)),
                MAX_ENTRIES,
            ),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn bump_generation(&self) {
        self.entries.bump_generation();
        self.totals.bump_generation();
        self.picks.bump_generation();
    }

    /// Single-flight page fetch: concurrent misses on one key wait for the leader.
    pub async fn page_or_fetch<F, Fut, E>(&self, key: Key, fetch: F) -> Result<Page, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Page, E>>,
    {
        if !self.enabled {
            return fetch().await;
        }
        self.entries.get_or_fetch(key, fetch).await
    }

    /// Anonymous pick stats carry no per-caller field, so one page's stats are shared by every
    /// uncredentialed caller until the favorites NOTIFY bumps the generation.
    pub async fn picks_or_fetch<F, Fut, E>(&self, ids: Vec<String>, fetch: F) -> Result<Picks, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Picks, E>>,
    {
        if !self.enabled {
            return fetch().await;
        }
        self.picks.get_or_fetch(ids, fetch).await
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

/// The count a page's `total` comes from depends on the predicates only, so the several pages
/// one render asks for (the shop's sort strips, a strip's page and its count) share it: one
/// query per predicate set per TTL, concurrent callers wait for the leader.
pub fn total_key(filters: &CatalogFilters) -> CatalogFilters {
    CatalogFilters {
        first: None,
        skip: None,
        sort_by: None,
        sort_direction: None,
        ..filters.clone()
    }
}

impl CatalogCache {
    pub async fn total_or_fetch<F, Fut, E>(
        &self,
        filters: &CatalogFilters,
        fetch: F,
    ) -> Result<i64, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<i64, E>>,
    {
        if !self.enabled {
            return fetch().await;
        }
        self.totals.get_or_fetch(total_key(filters), fetch).await
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

    #[tokio::test]
    async fn anonymous_picks_are_shared_until_a_bump() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = CatalogCache::new(60);
        let fetches = AtomicUsize::new(0);
        let fetch = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok::<Picks, ()>(Arc::new(Vec::new()))
        };
        let ids = vec!["0xc-1".to_string(), "0xc-2".to_string()];
        cache.picks_or_fetch(ids.clone(), fetch).await.unwrap();
        cache.picks_or_fetch(ids.clone(), fetch).await.unwrap();
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            1,
            "same page shares one query"
        );
        cache
            .picks_or_fetch(vec!["0xc-1".to_string()], fetch)
            .await
            .unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 2, "different ids miss");
        cache.bump_generation();
        cache.picks_or_fetch(ids, fetch).await.unwrap();
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            3,
            "a favorites NOTIFY invalidates"
        );
    }

    #[tokio::test]
    async fn totals_are_shared_across_page_shapes_and_bumped_with_pages() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = CatalogCache::new(60);
        let fetches = AtomicUsize::new(0);
        let count = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok::<i64, ()>(7_383)
        };
        let recently = CatalogFilters {
            first: Some(40),
            is_on_sale: Some(true),
            ..Default::default()
        };
        let cheapest = CatalogFilters {
            first: Some(8),
            skip: Some(0),
            sort_by: Some(super::super::catalog::CatalogSortBy::Cheapest),
            is_on_sale: Some(true),
            ..Default::default()
        };
        assert_eq!(cache.total_or_fetch(&recently, count).await, Ok(7_383));
        assert_eq!(cache.total_or_fetch(&cheapest, count).await, Ok(7_383));
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            1,
            "same predicates, one count"
        );

        let filtered = CatalogFilters {
            is_on_sale: Some(true),
            rarities: vec!["mythic".into()],
            ..Default::default()
        };
        assert_eq!(cache.total_or_fetch(&filtered, count).await, Ok(7_383));
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            2,
            "a new predicate counts again"
        );

        cache.bump_generation();
        assert_eq!(cache.total_or_fetch(&recently, count).await, Ok(7_383));
        assert_eq!(
            fetches.load(Ordering::SeqCst),
            3,
            "a write invalidates totals too"
        );
    }

    #[tokio::test]
    async fn totals_bypass_a_disabled_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = CatalogCache::new(0);
        let fetches = AtomicUsize::new(0);
        let count = || async {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok::<i64, ()>(1)
        };
        let f = CatalogFilters::default();
        assert_eq!(cache.total_or_fetch(&f, count).await, Ok(1));
        assert_eq!(cache.total_or_fetch(&f, count).await, Ok(1));
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
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
