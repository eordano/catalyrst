use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct EntityRateLimitConfig {
    pub ttl_ms: u64,
    pub max_size: usize,
    pub unchanged_ttl_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub default_ttl_ms: u64,
    pub default_max: usize,
    pub entity_configs: HashMap<String, EntityRateLimitConfig>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let mut entity_configs = HashMap::new();
        entity_configs.insert(
            "profile".into(),
            EntityRateLimitConfig {
                ttl_ms: 3_000,
                max_size: 500,
                unchanged_ttl_ms: 0,
            },
        );
        entity_configs.insert(
            "scene".into(),
            EntityRateLimitConfig {
                ttl_ms: 20_000,
                max_size: 1_000,
                unchanged_ttl_ms: 0,
            },
        );
        entity_configs.insert(
            "wearable".into(),
            EntityRateLimitConfig {
                ttl_ms: 20_000,
                max_size: 1_000,
                unchanged_ttl_ms: 0,
            },
        );
        entity_configs.insert(
            "store".into(),
            EntityRateLimitConfig {
                ttl_ms: 3_000,
                max_size: 300,
                unchanged_ttl_ms: 0,
            },
        );
        entity_configs.insert(
            "emote".into(),
            EntityRateLimitConfig {
                ttl_ms: 20_000,
                max_size: 1_000,
                unchanged_ttl_ms: 0,
            },
        );
        entity_configs.insert(
            "outfits".into(),
            EntityRateLimitConfig {
                ttl_ms: 3_000,
                max_size: 2_000,
                unchanged_ttl_ms: 0,
            },
        );
        Self {
            default_ttl_ms: 20_000,
            default_max: 1_000,
            entity_configs,
        }
    }
}

struct TtlCache {
    entries: HashMap<String, Instant>,
    ttl: Duration,
    max_size: usize,
}

impl TtlCache {
    fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            max_size,
        }
    }

    fn set(&mut self, key: String) {
        self.entries.insert(key, Instant::now());
    }

    fn get(&self, key: &str) -> bool {
        if let Some(ts) = self.entries.get(key) {
            ts.elapsed() < self.ttl
        } else {
            false
        }
    }

    fn len(&self) -> usize {
        self.entries.values().filter(|ts| ts.elapsed() < self.ttl).count()
    }

    fn is_max_size_hit(&self) -> bool {
        self.len() > self.max_size
    }
}

struct Inner {
    deployment_caches: HashMap<String, TtlCache>,
    unchanged_caches: HashMap<String, TtlCache>,
}

#[derive(Clone)]
pub struct DeployRateLimiter {
    inner: Arc<RwLock<Inner>>,
}

impl DeployRateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        let mut deployment_caches = HashMap::new();
        let mut unchanged_caches = HashMap::new();

        for (entity_type, ecfg) in &config.entity_configs {
            deployment_caches.insert(
                entity_type.clone(),
                TtlCache::new(Duration::from_millis(ecfg.ttl_ms), ecfg.max_size),
            );
            unchanged_caches.insert(
                entity_type.clone(),
                TtlCache::new(Duration::from_millis(ecfg.unchanged_ttl_ms), usize::MAX),
            );
        }

        Self {
            inner: Arc::new(RwLock::new(Inner {
                deployment_caches,
                unchanged_caches,
            })),
        }
    }

    pub async fn new_deployment(&self, entity_type: &str, pointers: &[String]) {
        let mut inner = self.inner.write().await;
        if let Some(cache) = inner.deployment_caches.get_mut(entity_type) {
            for p in pointers {
                cache.set(p.clone());
            }
        }
    }

    pub async fn is_rate_limited(&self, entity_type: &str, pointers: &[String]) -> bool {
        let inner = self.inner.read().await;
        if let Some(cache) = inner.deployment_caches.get(entity_type) {
            let ttl_hit = pointers.iter().any(|p| cache.get(p));
            let max_size_hit = cache.is_max_size_hit();
            ttl_hit || max_size_hit
        } else {
            false
        }
    }

    pub async fn new_unchanged_deployment(&self, entity_type: &str, pointers: &[String]) {
        let mut inner = self.inner.write().await;
        if let Some(cache) = inner.unchanged_caches.get_mut(entity_type) {
            for p in pointers {
                cache.set(p.clone());
            }
        }
    }

    pub async fn is_unchanged_deployment_rate_limited(
        &self,
        entity_type: &str,
        pointers: &[String],
    ) -> bool {
        let inner = self.inner.read().await;
        if let Some(cache) = inner.unchanged_caches.get(entity_type) {
            pointers.iter().any(|p| cache.get(p))
        } else {
            false
        }
    }
}
