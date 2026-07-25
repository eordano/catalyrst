use moka::future::Cache;
use std::time::Duration;

pub const MAX_ATTEMPTS_PER_MINUTE: u32 = 3;
pub const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
pub const RATE_LIMIT_TTL_SECONDS: u64 = 70;

#[derive(Clone)]
pub struct RateLimiter {
    attempts: Cache<String, u32>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Cache::builder()
                .time_to_live(Duration::from_secs(RATE_LIMIT_TTL_SECONDS))
                .max_capacity(100_000)
                .build(),
        }
    }

    fn key(world: &str, subject: &str) -> String {
        format!("{}:{}", world.to_lowercase(), subject.to_lowercase())
    }

    pub async fn is_rate_limited(&self, world: &str, subject: &str) -> bool {
        let count = self
            .attempts
            .get(&Self::key(world, subject))
            .await
            .unwrap_or(0);
        count >= MAX_ATTEMPTS_PER_MINUTE
    }

    pub async fn record_failed_attempt(&self, world: &str, subject: &str) -> bool {
        let key = Self::key(world, subject);
        let count = self.attempts.get(&key).await.unwrap_or(0) + 1;
        self.attempts.insert(key, count).await;
        count >= MAX_ATTEMPTS_PER_MINUTE
    }

    pub async fn clear_attempts(&self, world: &str, subject: &str) {
        self.attempts.invalidate(&Self::key(world, subject)).await;
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
