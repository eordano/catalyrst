use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use serde_json::Value;

const NAME_DENYLIST_TTL_SECONDS: u64 = 60 * 60;

#[derive(Clone)]
pub struct NameDenyListChecker {
    http: reqwest::Client,
    url: Option<String>,
    cache: Cache<(), Arc<Vec<String>>>,
}

impl NameDenyListChecker {
    pub fn new(http: reqwest::Client, url: Option<String>) -> Self {
        Self {
            http,
            url,
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(NAME_DENYLIST_TTL_SECONDS))
                .max_capacity(1)
                .build(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }

    async fn banned_names(&self) -> Arc<Vec<String>> {
        if let Some(cached) = self.cache.get(&()).await {
            return cached;
        }
        let fetched = Arc::new(fetch_banned_names(&self.http, self.url.as_deref()).await);
        self.cache.insert((), fetched.clone()).await;
        fetched
    }

    pub async fn check_name_deny_list(&self, world_name: &str) -> bool {
        !is_name_banned(&self.banned_names().await, world_name)
    }

    pub async fn get_banned_names(&self) -> Vec<String> {
        self.banned_names()
            .await
            .iter()
            .map(|n| strip_suffixes(n))
            .collect()
    }
}

async fn fetch_banned_names(http: &reqwest::Client, url: Option<&str>) -> Vec<String> {
    let Some(url) = url else {
        return Vec::new();
    };
    let endpoint = format!("{}/banned-names", url);
    match http.post(&endpoint).send().await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(body) => parse_banned_names(&body),
            Err(e) => {
                tracing::warn!(error = %e, url = %endpoint, "failed to parse name denylist (fail-open)");
                Vec::new()
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, url = %endpoint, "failed to fetch name denylist (fail-open)");
            Vec::new()
        }
    }
}

fn parse_banned_names(body: &Value) -> Vec<String> {
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|n| n.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_world_name(name: &str) -> String {
    name.to_lowercase()
        .replacen(".eth", "", 1)
        .replacen(".dcl", "", 1)
}

fn strip_suffixes(name: &str) -> String {
    name.replacen(".eth", "", 1).replacen(".dcl", "", 1)
}

fn is_name_banned(banned: &[String], world_name: &str) -> bool {
    let normalized = normalize_world_name(world_name);
    banned.iter().any(|name| name.to_lowercase() == normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_banned_names_reads_data_array() {
        let body = json!({ "data": ["foo", "bar", "baz"] });
        assert_eq!(parse_banned_names(&body), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn parse_banned_names_missing_or_malformed_is_empty() {
        assert!(parse_banned_names(&json!({})).is_empty());
        assert!(parse_banned_names(&json!({ "data": "nope" })).is_empty());
        assert!(parse_banned_names(&json!({ "data": [] })).is_empty());
    }

    #[test]
    fn normalize_strips_suffixes_and_lowercases() {
        assert_eq!(normalize_world_name("Foo.dcl.eth"), "foo");
        assert_eq!(normalize_world_name("BAR.eth"), "bar");
        assert_eq!(normalize_world_name("baz.dcl"), "baz");
        assert_eq!(normalize_world_name("Qux"), "qux");
    }

    #[test]
    fn is_name_banned_matches_case_insensitively_after_stripping() {
        let banned = vec!["foo".to_string(), "Evil".to_string()];
        assert!(is_name_banned(&banned, "foo.dcl.eth"));
        assert!(is_name_banned(&banned, "FOO.DCL.ETH"));
        assert!(is_name_banned(&banned, "evil.dcl.eth"));
        assert!(!is_name_banned(&banned, "good.dcl.eth"));
    }

    #[test]
    fn empty_banlist_allows_everything() {
        assert!(!is_name_banned(&[], "anything.dcl.eth"));
    }

    #[tokio::test]
    async fn unconfigured_checker_allows_all_names() {
        let checker = NameDenyListChecker::new(reqwest::Client::new(), None);
        assert!(!checker.is_configured());
        assert!(checker.check_name_deny_list("whatever.dcl.eth").await);
    }

    #[tokio::test]
    async fn seeded_checker_blocks_banned_name() {
        let checker = NameDenyListChecker::new(reqwest::Client::new(), None);
        checker
            .cache
            .insert((), Arc::new(vec!["banned".to_string()]))
            .await;

        assert!(!checker.check_name_deny_list("Banned.dcl.eth").await);
        assert!(checker.check_name_deny_list("fine.dcl.eth").await);
        assert_eq!(checker.get_banned_names().await, vec!["banned".to_string()]);
    }
}
