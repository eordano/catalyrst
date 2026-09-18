use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// first_wear re-reads `last_fetch_at` every 60 s (its POLL_SECS), so one
/// reader_seen write per address per window loses nothing.
pub const SEEN_WINDOW: Duration = Duration::from_secs(60);
pub const SEEN_MAX_ENTRIES: usize = 50_000;

pub struct SeenDebounce {
    max_entries: usize,
    inner: Mutex<HashMap<String, Instant>>,
}

impl Default for SeenDebounce {
    fn default() -> Self {
        Self::new(SEEN_MAX_ENTRIES)
    }
}

impl SeenDebounce {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn claim(&self, address: &str, now: Instant) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = map.get(address) {
            if now.duration_since(*at) < SEEN_WINDOW {
                return false;
            }
        }
        if map.len() >= self.max_entries && !map.contains_key(address) {
            map.retain(|_, at| now.duration_since(*at) < SEEN_WINDOW);
            if map.len() >= self.max_entries {
                map.clear();
            }
        }
        map.insert(address.to_string(), now);
        true
    }

    pub fn release(&self, address: &str) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(address);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_claim_per_address_per_window() {
        let seen = SeenDebounce::default();
        let t0 = Instant::now();
        assert!(seen.claim("0xa", t0));
        assert!(!seen.claim("0xa", t0));
        assert!(!seen.claim("0xa", t0 + SEEN_WINDOW - Duration::from_millis(1)));
        assert!(seen.claim("0xb", t0), "other addresses are independent");
        assert!(seen.claim("0xa", t0 + SEEN_WINDOW));
        assert!(!seen.claim("0xa", t0 + SEEN_WINDOW + Duration::from_secs(1)));
    }

    #[test]
    fn release_reopens_the_window_after_a_failed_write() {
        let seen = SeenDebounce::default();
        let t0 = Instant::now();
        assert!(seen.claim("0xa", t0));
        seen.release("0xa");
        assert!(seen.claim("0xa", t0));
    }

    #[test]
    fn map_stays_bounded() {
        let seen = SeenDebounce::new(4);
        let t0 = Instant::now();
        for i in 0..10 {
            assert!(seen.claim(&format!("0x{i}"), t0));
        }
        assert!(seen.len() <= 4, "grew to {}", seen.len());
        assert!(
            !seen.claim("0x9", t0),
            "the most recent claim survives eviction"
        );

        let seen = SeenDebounce::new(4);
        for i in 0..4 {
            assert!(seen.claim(&format!("0x{i}"), t0));
        }
        let later = t0 + SEEN_WINDOW;
        assert!(seen.claim("0xnew", later));
        assert_eq!(seen.len(), 1, "stale entries are shed before clearing");
    }
}
