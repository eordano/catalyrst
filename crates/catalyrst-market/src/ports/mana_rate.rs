use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;

pub const DEFAULT_PRICE_BASE_URL: &str = "http://127.0.0.1:5156";
pub const DEFAULT_REFRESH_INTERVAL_MS: u64 = 90_000;
pub const DEFAULT_FALLBACK_RATE: f64 = 0.02;
pub const DEFAULT_MAX_STALENESS_SECONDS: i64 = 86_400;
pub const DEFAULT_STARTUP_TIMEOUT_MS: u64 = 5_000;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

struct Inner {
    http: reqwest::Client,
    base_url: String,
    fallback_rate: f64,
    max_staleness_secs: i64,
    cached: RwLock<Option<f64>>,
}

#[derive(Clone)]
pub struct ManaUsdRateComponent {
    inner: Arc<Inner>,
}

impl ManaUsdRateComponent {
    pub fn new(base_url: String, fallback_rate: f64, max_staleness_secs: i64) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                http,
                base_url,
                fallback_rate,
                max_staleness_secs,
                cached: RwLock::new(None),
            }),
        }
    }

    pub fn get_rate(&self) -> f64 {
        (*self.inner.cached.read()).unwrap_or(self.inner.fallback_rate)
    }

    pub async fn refresh(&self) {
        match self.fetch_rate().await {
            Ok(rate) => {
                *self.inner.cached.write() = Some(rate);
                tracing::debug!(rate, "MANA/USD rate refreshed");
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    keeping = self.get_rate(),
                    "failed to refresh MANA/USD rate, keeping last-known/fallback"
                );
            }
        }
    }

    async fn fetch_rate(&self) -> Result<f64, String> {
        let url = format!("{}/api/v3/simple/price", self.inner.base_url);
        let resp = self
            .inner
            .http
            .get(&url)
            .query(&[
                ("ids", "decentraland"),
                ("vs_currencies", "usd"),
                ("include_last_updated_at", "true"),
            ])
            .send()
            .await
            .map_err(|e| format!("price oracle request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "price oracle returned status {}",
                resp.status().as_u16()
            ));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("price oracle parse failed: {e}"))?;
        parse_simple_price(
            &body,
            chrono::Utc::now().timestamp(),
            self.inner.max_staleness_secs,
        )
    }

    pub async fn start(&self, startup_timeout: Duration, refresh_interval: Duration) {
        let initial = {
            let this = self.clone();
            tokio::spawn(async move { this.refresh().await })
        };
        let _ = tokio::time::timeout(startup_timeout, initial).await;

        let this = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                this.refresh().await;
            }
        });
    }

    #[cfg(test)]
    pub(crate) fn store_rate(&self, rate: f64) {
        *self.inner.cached.write() = Some(rate);
    }
}

pub fn parse_simple_price(
    body: &serde_json::Value,
    now_s: i64,
    max_staleness_secs: i64,
) -> Result<f64, String> {
    let mana = body
        .get("decentraland")
        .ok_or("oracle response missing 'decentraland'")?;
    let usd = mana
        .get("usd")
        .and_then(|v| v.as_f64())
        .ok_or("oracle response missing numeric 'usd'")?;
    if !usd.is_finite() || usd <= 0.0 {
        return Err(format!(
            "oracle returned a non-positive MANA/USD rate ({usd})"
        ));
    }
    let last_updated_at = mana
        .get("last_updated_at")
        .and_then(|v| v.as_i64())
        .ok_or("oracle response missing 'last_updated_at'")?;
    let age = now_s - last_updated_at;
    if age > max_staleness_secs {
        return Err(format!(
            "MANA/USD oracle is stale (age {age}s exceeds {max_staleness_secs}s)"
        ));
    }
    Ok(usd)
}

pub fn rate_to_numeric_string(rate: f64) -> String {
    if !rate.is_finite() || rate <= 0.0 {
        return "0".to_string();
    }
    format!("{rate:.18}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_fresh_positive_rate() {
        let body = json!({"decentraland": {"usd": 0.025, "last_updated_at": 1_000}});
        assert_eq!(parse_simple_price(&body, 1_100, 300), Ok(0.025));
    }

    #[test]
    fn staleness_bound_is_inclusive_and_tolerates_future_timestamps() {
        let body = json!({"decentraland": {"usd": 0.025, "last_updated_at": 1_000}});
        assert!(parse_simple_price(&body, 1_300, 300).is_ok());
        assert!(parse_simple_price(&body, 1_301, 300)
            .unwrap_err()
            .contains("stale"));
        assert!(parse_simple_price(&body, 900, 300).is_ok());
    }

    #[test]
    fn refuses_non_positive_or_missing_rates() {
        for body in [
            json!({}),
            json!({"decentraland": {}}),
            json!({"decentraland": {"usd": "0.02", "last_updated_at": 1_000}}),
            json!({"decentraland": {"usd": 0.0, "last_updated_at": 1_000}}),
            json!({"decentraland": {"usd": -1.0, "last_updated_at": 1_000}}),
            json!({"decentraland": {"usd": 0.02}}),
        ] {
            assert!(parse_simple_price(&body, 1_100, 300).is_err(), "{body}");
        }
    }

    #[test]
    fn get_rate_falls_back_then_serves_the_cached_rate() {
        let c = ManaUsdRateComponent::new("http://127.0.0.1:1".to_string(), 0.02, 300);
        assert_eq!(c.get_rate(), 0.02);
        c.store_rate(0.031);
        assert_eq!(c.get_rate(), 0.031);
    }

    #[test]
    fn rate_to_numeric_string_bounds_precision_and_zeroes_bad_rates() {
        assert_eq!(rate_to_numeric_string(0.5), "0.500000000000000000");
        assert_eq!(rate_to_numeric_string(0.02), "0.020000000000000000");
        assert_eq!(rate_to_numeric_string(0.0), "0");
        assert_eq!(rate_to_numeric_string(-0.5), "0");
        assert_eq!(rate_to_numeric_string(f64::NAN), "0");
        assert_eq!(rate_to_numeric_string(f64::INFINITY), "0");
    }
}
