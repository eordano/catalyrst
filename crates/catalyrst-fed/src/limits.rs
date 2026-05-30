use dashmap::DashMap;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum RateLimitDecision {
    Allow,
    Deny,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: DashMap<String, Mutex<Bucket>>,
}

impl RateLimiter {
    pub fn new(capacity: u32, per: Duration) -> Self {
        let refill_per_sec = capacity as f64 / per.as_secs_f64();
        Self {
            capacity: capacity as f64,
            refill_per_sec,
            buckets: DashMap::new(),
        }
    }

    pub fn check(&self, signer: &str) -> RateLimitDecision {
        let now = Instant::now();
        let entry = self.buckets.entry(signer.to_ascii_lowercase()).or_insert_with(|| {
            Mutex::new(Bucket { tokens: self.capacity, last: now })
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
}
