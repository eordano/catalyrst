use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Small TTL memo: fresh hits clone out, expired entries are dropped lazily when full.
pub struct TtlMemo<K, V> {
    ttl: Duration,
    max_entries: usize,
    map: Mutex<HashMap<K, (Instant, V)>>,
}

impl<K: Eq + Hash, V: Clone> TtlMemo<K, V> {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries: max_entries.max(1),
            map: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let map = self.map.lock().ok()?;
        let (at, value) = map.get(key)?;
        (at.elapsed() < self.ttl).then(|| value.clone())
    }

    pub fn insert(&self, key: K, value: V) {
        let Ok(mut map) = self.map.lock() else {
            return;
        };
        if map.len() >= self.max_entries && !map.contains_key(&key) {
            let ttl = self.ttl;
            map.retain(|_, (at, _)| at.elapsed() < ttl);
            if map.len() >= self.max_entries {
                map.clear();
            }
        }
        map.insert(key, (Instant::now(), value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_hits_and_bounded_growth() {
        let memo: TtlMemo<u8, u8> = TtlMemo::new(Duration::from_secs(60), 2);
        assert_eq!(memo.get(&1), None);
        memo.insert(1, 10);
        memo.insert(2, 20);
        memo.insert(3, 30);
        assert_eq!(memo.get(&3), Some(30));
        assert!(memo.map.lock().unwrap().len() <= 2);
    }

    #[test]
    fn expired_entries_are_misses() {
        let memo: TtlMemo<u8, u8> = TtlMemo::new(Duration::ZERO, 4);
        memo.insert(1, 10);
        assert_eq!(memo.get(&1), None);
    }
}
