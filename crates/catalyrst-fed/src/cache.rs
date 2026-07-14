use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Cached<T> {
    pub value: T,
    pub expires_at: Instant,
}

pub fn cache_get<T: Clone>(cache: &Mutex<HashMap<String, Cached<T>>>, key: &str) -> Option<T> {
    let cache = cache.lock().unwrap();
    cache
        .get(key)
        .filter(|c| c.expires_at > Instant::now())
        .map(|c| c.value.clone())
}

pub fn cache_put<T>(
    cache: &Mutex<HashMap<String, Cached<T>>>,
    key: String,
    value: T,
    ttl: Duration,
) {
    let mut cache = cache.lock().unwrap();
    let now = Instant::now();
    cache.retain(|_, c| c.expires_at > now);
    cache.insert(
        key,
        Cached {
            value,
            expires_at: now + ttl,
        },
    );
}
