use std::future::Future;
use std::time::Duration;

use anyhow::Result;

pub enum RetryDecision<T> {
    Done(T),
    Retry(Option<Duration>),
}

pub async fn retry_with_backoff<T, F, Fut>(
    label: &str,
    attempts: u32,
    base_ms: u64,
    mut op: F,
) -> Result<Option<T>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<RetryDecision<T>>>,
{
    let mut backoff_ms = base_ms;
    for attempt in 1..=attempts {
        match op().await? {
            RetryDecision::Done(v) => return Ok(Some(v)),
            RetryDecision::Retry(suggested) => {
                if attempt == attempts {
                    return Ok(None);
                }
                let wait = suggested.unwrap_or_else(|| Duration::from_millis(backoff_ms));
                eprintln!(
                    "  ~ {} transient failure (attempt {}/{}), sleeping {}ms",
                    label,
                    attempt,
                    attempts,
                    wait.as_millis()
                );
                tokio::time::sleep(wait).await;
                backoff_ms = backoff_ms.saturating_mul(2);
            }
        }
    }
    Ok(None)
}

pub fn parse_retry_after(header: &str) -> Option<Duration> {
    let trimmed = header.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    if !trimmed.is_empty() {
        return Some(Duration::from_secs(5));
    }
    None
}

pub fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}
