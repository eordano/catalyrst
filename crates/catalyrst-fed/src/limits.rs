use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const DEFAULT_MAX_BUCKETS: usize = 10_000;

const SWEEP_EVERY: u64 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allow,
    Deny,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Token-bucket limiter keyed by an arbitrary caller-supplied string.
///
/// Keys are attacker-chosen on public routes, so the map is pruned: a fully refilled
/// bucket carries no state worth keeping and is dropped on a periodic sweep, and the map
/// is hard-capped so a burst of distinct keys cannot grow the process without bound.
pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    max_buckets: usize,
    checks: AtomicU64,
    buckets: DashMap<String, Mutex<Bucket>>,
}

impl RateLimiter {
    pub fn new(capacity: u32, per: Duration) -> Self {
        let refill_per_sec = capacity as f64 / per.as_secs_f64();
        Self {
            capacity: capacity as f64,
            refill_per_sec,
            max_buckets: DEFAULT_MAX_BUCKETS,
            checks: AtomicU64::new(0),
            buckets: DashMap::new(),
        }
    }

    pub fn with_max_buckets(mut self, max_buckets: usize) -> Self {
        self.max_buckets = max_buckets.max(1);
        self
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Forgives a key's accumulated deficit. Called when the caller proves it is
    /// legitimate, so earlier failures cannot lock out a user who then gets it right.
    pub fn clear(&self, signer: &str) {
        self.buckets.remove(&signer.to_ascii_lowercase());
    }

    fn maybe_sweep(&self) {
        let n = self.checks.fetch_add(1, Ordering::Relaxed);
        if n.is_multiple_of(SWEEP_EVERY) || self.buckets.len() >= self.max_buckets {
            self.sweep();
        }
    }

    fn sweep(&self) {
        let now = Instant::now();
        let capacity = self.capacity;
        let refill_per_sec = self.refill_per_sec;
        self.buckets.retain(|_, bucket| {
            let b = bucket.get_mut();
            let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
            b.tokens + elapsed * refill_per_sec < capacity
        });
        if self.buckets.len() >= self.max_buckets {
            self.buckets.clear();
        }
    }

    pub fn check(&self, signer: &str) -> RateLimitDecision {
        self.maybe_sweep();
        let now = Instant::now();
        let entry = self
            .buckets
            .entry(signer.to_ascii_lowercase())
            .or_insert_with(|| {
                Mutex::new(Bucket {
                    tokens: self.capacity,
                    last: now,
                })
            });
        let mut b = entry.lock();
        let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            RateLimitDecision::Allow
        } else {
            RateLimitDecision::Deny
        }
    }

    pub fn is_rate_limited(&self, signer: &str) -> bool {
        let key = signer.to_ascii_lowercase();
        match self.buckets.get(&key) {
            Some(entry) => {
                let b = entry.lock();
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
                let tokens = (b.tokens + elapsed * self.refill_per_sec).min(self.capacity);
                tokens < 1.0
            }
            None => false,
        }
    }

    pub fn record_failed_attempt(&self, signer: &str) -> RateLimitDecision {
        self.check(signer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_signer_is_not_rate_limited() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(!limiter.is_rate_limited("0xabc"));
    }

    #[test]
    fn is_rate_limited_does_not_consume_tokens() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(matches!(limiter.check("0xabc"), RateLimitDecision::Allow));

        for _ in 0..10 {
            assert!(!limiter.is_rate_limited("0xabc"));
        }

        assert!(matches!(limiter.check("0xabc"), RateLimitDecision::Allow));
        assert!(limiter.is_rate_limited("0xabc"));
    }

    #[test]
    fn record_failed_attempt_consumes_like_check() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(!limiter.is_rate_limited("0xabc"));
        assert!(matches!(
            limiter.record_failed_attempt("0xabc"),
            RateLimitDecision::Allow
        ));
        assert!(limiter.is_rate_limited("0xabc"));
        assert!(matches!(
            limiter.record_failed_attempt("0xabc"),
            RateLimitDecision::Deny
        ));
    }

    #[test]
    fn is_rate_limited_matches_check_key_normalization() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(matches!(limiter.check("0xABC"), RateLimitDecision::Allow));
        assert!(limiter.is_rate_limited("0xabc"));
    }

    #[test]
    fn clear_forgives_a_keys_deficit() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(matches!(
            limiter.record_failed_attempt("0xABC"),
            RateLimitDecision::Allow
        ));
        assert!(limiter.is_rate_limited("0xabc"));
        limiter.clear("0xabc");
        assert!(!limiter.is_rate_limited("0xABC"));
    }

    #[test]
    fn distinct_keys_never_grow_past_the_cap() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60)).with_max_buckets(64);
        for i in 0..10_000 {
            limiter.check(&format!("ip-{i}"));
        }
        assert!(
            limiter.len() <= 64,
            "attacker-keyed buckets grew to {}",
            limiter.len()
        );
    }

    #[test]
    fn a_refilled_bucket_is_swept_but_a_deficient_one_survives() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(matches!(limiter.check("full"), RateLimitDecision::Allow));
        {
            let entry = limiter.buckets.get("full").expect("bucket must exist");
            let mut b = entry.lock();
            b.tokens = 2.0;
        }
        assert!(matches!(limiter.check("spent"), RateLimitDecision::Allow));
        assert!(matches!(limiter.check("spent"), RateLimitDecision::Allow));

        limiter.sweep();
        assert!(limiter.buckets.get("full").is_none());
        assert!(limiter.buckets.get("spent").is_some());
    }

    #[test]
    fn a_burst_of_fresh_keys_is_allowed_while_the_cap_holds() {
        let limiter = RateLimiter::new(10, Duration::from_secs(60)).with_max_buckets(3);
        for i in 0..5 {
            assert_eq!(
                limiter.check(&format!("signer{i}")),
                RateLimitDecision::Allow,
                "fresh key must be allowed"
            );
        }
        assert!(
            limiter.len() <= 3,
            "cap must hold after a burst of fresh keys"
        );
        assert!(limiter.buckets.contains_key("signer4"));
        assert!(!limiter.buckets.contains_key("signer0"));
        assert_eq!(limiter.check("signer0"), RateLimitDecision::Allow);
        assert!(limiter.len() <= 3);
    }

    #[test]
    fn a_swept_key_starts_from_a_full_bucket() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert_eq!(limiter.check("a"), RateLimitDecision::Allow);
        assert_eq!(limiter.check("a"), RateLimitDecision::Allow);
        assert_eq!(limiter.check("a"), RateLimitDecision::Deny);
        {
            let entry = limiter.buckets.get("a").expect("bucket must exist");
            let mut b = entry.lock();
            b.tokens = 2.0;
        }
        limiter.sweep();
        assert!(limiter.is_empty());
        assert_eq!(limiter.check("a"), RateLimitDecision::Allow);
        assert_eq!(limiter.len(), 1);
    }
}
