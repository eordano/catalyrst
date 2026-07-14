use std::future::Future;
use std::time::Duration;

use anyhow::Result;

pub enum RetryDecision<T> {
    Done(T),
    Retry {
        after: Option<Duration>,
        rate_limited: bool,
    },
}

/// Rate-limited (429) responses get their own, longer retry ladder: public
/// peers (nginx behind Cloudflare) throttle /content/deployments with a
/// multi-second penalty window and send no Retry-After header, so the
/// caller-supplied schedule (typically 1s/2s) always lands inside the window.
/// The 2s/4s/8s/16s ladder waits up to 30s cumulatively, comfortably past the
/// ~7s recovery observed against peer.decentraland.org.
const RATE_LIMIT_ATTEMPTS: u32 = 5;
const RATE_LIMIT_BASE_MS: u64 = 2000;
/// Defensive cap on any single sleep (e.g. a huge Retry-After header).
const MAX_SLEEP: Duration = Duration::from_secs(30);

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
    let max_attempts = attempts.max(RATE_LIMIT_ATTEMPTS);
    let mut backoff_ms = base_ms;
    let mut rl_backoff_ms = RATE_LIMIT_BASE_MS;
    let mut plain_failures = 0u32;
    for attempt in 1..=max_attempts {
        match op().await? {
            RetryDecision::Done(v) => return Ok(Some(v)),
            RetryDecision::Retry {
                after,
                rate_limited,
            } => {
                if !rate_limited {
                    plain_failures += 1;
                    // Plain transient failures (network errors, 5xx) still
                    // give up per the caller's budget.
                    if plain_failures >= attempts {
                        return Ok(None);
                    }
                }
                if attempt == max_attempts {
                    return Ok(None);
                }
                let ladder = if rate_limited {
                    let d = Duration::from_millis(rl_backoff_ms);
                    rl_backoff_ms = rl_backoff_ms.saturating_mul(2);
                    d
                } else {
                    let d = Duration::from_millis(backoff_ms);
                    backoff_ms = backoff_ms.saturating_mul(2);
                    d
                };
                let wait = after.unwrap_or(ladder).min(MAX_SLEEP);
                eprintln!(
                    "  ~ {} {} (attempt {}/{}), sleeping {}ms",
                    label,
                    if rate_limited {
                        "rate-limited (429)"
                    } else {
                        "transient failure"
                    },
                    attempt,
                    if rate_limited { max_attempts } else { attempts },
                    wait.as_millis()
                );
                tokio::time::sleep(wait).await;
            }
        }
    }
    Ok(None)
}

pub fn parse_retry_after(header: &str) -> Option<Duration> {
    let trimmed = header.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(trimmed) {
        let delta = date.with_timezone(&chrono::Utc) - chrono::Utc::now();
        return Some(delta.to_std().unwrap_or(Duration::ZERO));
    }
    Some(Duration::from_secs(5))
}

pub fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn parse_retry_after_integer_seconds() {
        assert_eq!(parse_retry_after("3"), Some(Duration::from_secs(3)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
    }

    #[test]
    fn parse_retry_after_http_date_in_future() {
        let future = (chrono::Utc::now() + chrono::Duration::seconds(10)).to_rfc2822();
        let d = parse_retry_after(&future).expect("future date should parse");
        assert!(
            d > Duration::from_secs(8) && d <= Duration::from_secs(10),
            "expected ~10s, got {:?}",
            d
        );
    }

    #[test]
    fn parse_retry_after_http_date_in_past() {
        let past = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc2822();
        assert_eq!(parse_retry_after(&past), Some(Duration::ZERO));
    }

    #[test]
    fn parse_retry_after_garbage_falls_back() {
        assert_eq!(parse_retry_after("garbage"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_retry_after_empty_is_none() {
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("   "), None);
    }

    /// Runs `retry_with_backoff` against an op that always asks for a retry,
    /// returning the (paused-clock) elapsed time at each attempt.
    async fn drive_schedule(
        attempts: u32,
        base_ms: u64,
        after: Option<Duration>,
        rate_limited: bool,
    ) -> Vec<Duration> {
        let times: Mutex<Vec<Duration>> = Mutex::new(Vec::new());
        let times_ref = &times;
        let start = tokio::time::Instant::now();
        let result: Option<()> =
            retry_with_backoff("test", attempts, base_ms, move || async move {
                times_ref.lock().unwrap().push(start.elapsed());
                Ok(RetryDecision::Retry {
                    after,
                    rate_limited,
                })
            })
            .await
            .unwrap();
        assert!(result.is_none());
        times.into_inner().unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_ladder_is_2_4_8_16_over_5_attempts() {
        let times = drive_schedule(3, 1000, None, true).await;
        // Attempts at t = 0, 2, 6, 14, 30 seconds (sleeps of 2/4/8/16s).
        let expected: Vec<Duration> = [0u64, 2, 6, 14, 30]
            .iter()
            .map(|s| Duration::from_secs(*s))
            .collect();
        assert_eq!(times, expected);
    }

    #[tokio::test(start_paused = true)]
    async fn plain_transient_keeps_caller_schedule() {
        let times = drive_schedule(3, 1000, None, false).await;
        // Attempts at t = 0, 1, 3 seconds (sleeps of 1s/2s), then give up.
        let expected: Vec<Duration> = [0u64, 1, 3]
            .iter()
            .map(|s| Duration::from_secs(*s))
            .collect();
        assert_eq!(times, expected);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limited_honours_retry_after_hint() {
        let times = drive_schedule(3, 1000, Some(Duration::from_secs(7)), true).await;
        assert_eq!(times.len(), 5);
        assert_eq!(times[1], Duration::from_secs(7));
        assert_eq!(times[2], Duration::from_secs(14));
    }

    #[tokio::test(start_paused = true)]
    async fn sleeps_are_clamped_to_30s() {
        let times = drive_schedule(3, 1000, Some(Duration::from_secs(120)), true).await;
        assert_eq!(times[1], Duration::from_secs(30));
    }

    #[tokio::test(start_paused = true)]
    async fn done_short_circuits() {
        let result =
            retry_with_backoff("test", 3, 1000, || async { Ok(RetryDecision::Done(42u32)) })
                .await
                .unwrap();
        assert_eq!(result, Some(42));
    }
}
