use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moka::future::Cache;
use serde_json::Value;

const DENYLIST_TTL_SECONDS: u64 = 5 * 60;

#[derive(Clone)]
pub struct DenyListComponent {
    http: reqwest::Client,
    url: Option<String>,
    cache: Cache<(), Arc<HashSet<String>>>,
    last_good: Arc<Mutex<Arc<HashSet<String>>>>,
}

impl DenyListComponent {
    pub fn new(http: reqwest::Client, url: Option<String>) -> Self {
        Self {
            http,
            url,
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(DENYLIST_TTL_SECONDS))
                .max_capacity(1)
                .build(),
            last_good: Arc::new(Mutex::new(Arc::new(HashSet::new()))),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }

    async fn wallets(&self) -> Arc<HashSet<String>> {
        if let Some(cached) = self.cache.get(&()).await {
            return cached;
        }
        match fetch_denylist(&self.http, self.url.as_deref()).await {
            Some(set) => {
                let fetched = Arc::new(set);
                self.cache.insert((), fetched.clone()).await;
                *self.last_good.lock().unwrap() = fetched.clone();
                fetched
            }
            None => self.last_good.lock().unwrap().clone(),
        }
    }

    pub async fn is_denylisted(&self, identity: &str) -> bool {
        contains_wallet(self.wallets().await.as_ref(), identity)
    }
}

async fn fetch_denylist(http: &reqwest::Client, url: Option<&str>) -> Option<HashSet<String>> {
    let url = url?;
    match http.get(url).send().await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(body) => Some(parse_denylist(&body)),
            Err(_) => {
                tracing::warn!("failed to parse wallet denylist; keeping last known list");
                None
            }
        },
        Err(_) => {
            tracing::warn!("failed to fetch wallet denylist; keeping last known list");
            None
        }
    }
}

fn parse_denylist(body: &Value) -> HashSet<String> {
    body.get("users")
        .and_then(|u| u.as_array())
        .map(|users| {
            users
                .iter()
                .filter_map(|u| u.get("wallet").and_then(|w| w.as_str()))
                .map(|w| w.to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn contains_wallet(wallets: &HashSet<String>, identity: &str) -> bool {
    wallets.contains(&identity.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[derive(Clone, Default)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for LogBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wallet_denylist_transport_logs_never_retain_the_configured_url() {
        const CANARY: &str = "worlds-wallet-denylist-private-material-canary";
        let component = DenyListComponent::new(
            reqwest::Client::new(),
            Some(format!("http://127.0.0.1:1/{CANARY}")),
        );
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        assert!(!component.is_denylisted("0xabc").await);

        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("failed to fetch wallet denylist"));
        assert!(!logs.contains(CANARY), "configured URL survived in {logs}");
    }

    #[test]
    fn parse_denylist_lowercases_wallets() {
        let body = json!({
            "users": [
                { "wallet": "0xABC123" },
                { "wallet": "0xDeadBeef" }
            ]
        });
        let set = parse_denylist(&body);
        assert_eq!(set.len(), 2);
        assert!(set.contains("0xabc123"));
        assert!(set.contains("0xdeadbeef"));
        assert!(!set.contains("0xABC123"));
    }

    #[test]
    fn parse_denylist_missing_or_malformed_is_empty() {
        assert!(parse_denylist(&json!({})).is_empty());
        assert!(parse_denylist(&json!({ "users": "not-an-array" })).is_empty());
        assert!(parse_denylist(&json!({ "users": [] })).is_empty());
        assert!(parse_denylist(&json!({ "users": [ { "foo": "bar" } ] })).is_empty());
    }

    #[test]
    fn contains_wallet_is_case_insensitive() {
        let mut set = HashSet::new();
        set.insert("0xabc".to_string());
        assert!(contains_wallet(&set, "0xABC"));
        assert!(contains_wallet(&set, "0xabc"));
        assert!(!contains_wallet(&set, "0xdef"));
    }

    #[tokio::test]
    async fn unconfigured_denylist_never_denies() {
        let comp = DenyListComponent::new(reqwest::Client::new(), None);
        assert!(!comp.is_configured());
        assert!(!comp.is_denylisted("0xabc").await);
    }

    #[tokio::test]
    async fn seeded_denylist_denies_case_insensitively() {
        let comp = DenyListComponent::new(reqwest::Client::new(), None);
        let mut set = HashSet::new();
        set.insert("0xbanned".to_string());
        comp.cache.insert((), Arc::new(set)).await;

        assert!(comp.is_denylisted("0xBANNED").await);
        assert!(!comp.is_denylisted("0xok").await);
    }
}
