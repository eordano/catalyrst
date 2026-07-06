use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Deserialize;

const BAN_CHECK_TIMEOUT: Duration = Duration::from_millis(1000);

const DENY_LIST_TTL: Duration = Duration::from_secs(5 * 60);

pub fn normalize_address(address: &str) -> String {
    address.to_ascii_lowercase()
}

pub fn encode_uri_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            );
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b & 0x0f));
        }
    }
    out
}

fn hex_upper(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

#[derive(Deserialize)]
struct BansEnvelope {
    #[serde(default)]
    data: Option<BansData>,
}

#[derive(Deserialize)]
struct BansData {
    #[serde(rename = "isBanned", default)]
    is_banned: Option<bool>,
}

#[derive(Clone)]
pub struct BanChecker {
    gatekeeper_url: Option<String>,
    http: reqwest::Client,
}

impl BanChecker {
    pub fn new(gatekeeper_url: Option<String>, http: reqwest::Client) -> Arc<Self> {
        let gatekeeper_url = gatekeeper_url
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty());
        Arc::new(Self {
            gatekeeper_url,
            http,
        })
    }

    pub fn is_armed(&self) -> bool {
        self.gatekeeper_url.is_some()
    }

    pub async fn is_banned(&self, address: &str) -> bool {
        let Some(base) = self.gatekeeper_url.as_deref() else {
            return false;
        };
        let url = format!("{}/users/{}/bans", base, encode_uri_component(address));
        let resp = match self.http.get(&url).timeout(BAN_CHECK_TIMEOUT).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(address, error = %e, "ban check failed, allowing connection");
                return false;
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(address, status = %resp.status(), "ban check non-OK status, allowing connection");
            return false;
        }
        match resp.json::<BansEnvelope>().await {
            Ok(body) => body.data.and_then(|d| d.is_banned).unwrap_or(false),
            Err(e) => {
                tracing::warn!(address, error = %e, "ban check malformed body, allowing connection");
                false
            }
        }
    }
}

#[derive(Deserialize)]
struct DenyListDoc {
    #[serde(default)]
    users: Option<Vec<DenyUser>>,
}

#[derive(Deserialize)]
struct DenyUser {
    #[serde(default)]
    wallet: Option<String>,
}

struct DenyListCache {
    wallets: HashSet<String>,
    last_fetched: Option<Instant>,
}

pub struct DenyList {
    url: Option<String>,
    http: reqwest::Client,
    ttl: Duration,
    cache: Mutex<DenyListCache>,
}

impl DenyList {
    pub fn new(url: Option<String>, http: reqwest::Client) -> Arc<Self> {
        Self::with_ttl(url, http, DENY_LIST_TTL)
    }

    pub fn with_ttl(url: Option<String>, http: reqwest::Client, ttl: Duration) -> Arc<Self> {
        let url = url.filter(|u| !u.is_empty());
        Arc::new(Self {
            url,
            http,
            ttl,
            cache: Mutex::new(DenyListCache {
                wallets: HashSet::new(),
                last_fetched: None,
            }),
        })
    }

    pub fn is_armed(&self) -> bool {
        self.url.is_some()
    }

    pub async fn is_denied(&self, address: &str) -> bool {
        if self.url.is_none() {
            return false;
        }
        let wallets = self.current().await;
        wallets.contains(&normalize_address(address))
    }

    async fn current(&self) -> HashSet<String> {
        {
            let cache = self.cache.lock();
            if let Some(at) = cache.last_fetched {
                if at.elapsed() < self.ttl {
                    return cache.wallets.clone();
                }
            }
        }
        let fetched = self.fetch().await;
        let mut cache = self.cache.lock();
        if let Some(wallets) = fetched {
            cache.wallets = wallets;
        }
        cache.last_fetched = Some(Instant::now());
        cache.wallets.clone()
    }

    async fn fetch(&self) -> Option<HashSet<String>> {
        let url = self.url.as_deref()?;
        let resp = match self.http.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "deny list fetch failed, keeping last known list");
                return None;
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "deny list non-OK status, keeping last known list");
            return None;
        }
        let doc = match resp.json::<DenyListDoc>().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "deny list malformed body, keeping last known list");
                return None;
            }
        };
        match doc.users {
            Some(users) => Some(
                users
                    .into_iter()
                    .filter_map(|u| u.wallet)
                    .map(|w| normalize_address(&w))
                    .collect(),
            ),
            None => {
                tracing::warn!("deny list missing 'users' field, treating as empty");
                Some(HashSet::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_uri_component_matches_js_for_special_chars() {
        assert_eq!(
            encode_uri_component("user/with?special#chars"),
            "user%2Fwith%3Fspecial%23chars"
        );
    }

    #[test]
    fn encode_uri_component_leaves_hex_addresses_untouched() {
        let addr = "0xAbC0123456789def";
        assert_eq!(encode_uri_component(addr), addr);
    }

    #[test]
    fn encode_uri_component_keeps_unreserved_marks() {
        assert_eq!(encode_uri_component("-_.!~*'()"), "-_.!~*'()");
    }

    #[test]
    fn encode_uri_component_escapes_space_and_utf8_bytes() {
        assert_eq!(encode_uri_component("a b"), "a%20b");
        assert_eq!(encode_uri_component("é"), "%C3%A9");
    }

    #[test]
    fn normalize_address_lowercases() {
        assert_eq!(normalize_address("0xABCdef"), "0xabcdef");
    }

    #[tokio::test]
    async fn disarmed_ban_checker_never_calls_out_and_allows() {
        let checker = BanChecker::new(None, reqwest::Client::new());
        assert!(!checker.is_armed());
        assert!(!checker.is_banned("0xdeadbeef").await);
    }

    #[tokio::test]
    async fn empty_gatekeeper_url_is_treated_as_disarmed() {
        let checker = BanChecker::new(Some(String::new()), reqwest::Client::new());
        assert!(!checker.is_armed());
        assert!(!checker.is_banned("0xabc").await);
    }

    #[tokio::test]
    async fn disarmed_deny_list_allows_everyone() {
        let deny = DenyList::new(None, reqwest::Client::new());
        assert!(!deny.is_armed());
        assert!(!deny.is_denied("0xdeadbeef").await);
    }
}
