use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moka::future::Cache;
use serde_json::Value;

const NAME_DENYLIST_TTL_SECONDS: u64 = 60 * 60;

/// The ban list, normalized once at fetch time instead of per entry per check.
///
/// `raw` is what the endpoint returned, kept for the admin display path. `normalized` is
/// the same entries lowercased and ONLY lowercased, not suffix-stripped: the pre-existing
/// semantics compared `entry.to_lowercase()` against `normalize_world_name(world_name)`.
pub(crate) struct BannedNames {
    raw: Vec<String>,
    normalized: HashSet<String>,
}

impl BannedNames {
    fn from_list(names: Vec<String>) -> Self {
        let normalized = names.iter().map(|n| n.to_lowercase()).collect();
        Self {
            raw: names,
            normalized,
        }
    }
}

#[derive(Clone)]
pub struct NameDenyListChecker {
    http: reqwest::Client,
    url: Option<String>,
    cache: Cache<(), Arc<BannedNames>>,
    last_good: Arc<Mutex<Arc<BannedNames>>>,
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
            last_good: Arc::new(Mutex::new(Arc::new(BannedNames::from_list(Vec::new())))),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }

    async fn banned_names(&self) -> Arc<BannedNames> {
        if let Some(cached) = self.cache.get(&()).await {
            return cached;
        }
        match fetch_banned_names(&self.http, self.url.as_deref()).await {
            Some(names) => {
                let fetched = Arc::new(BannedNames::from_list(names));
                self.cache.insert((), fetched.clone()).await;
                *self.last_good.lock().unwrap() = fetched.clone();
                fetched
            }
            None => self.last_good.lock().unwrap().clone(),
        }
    }

    pub async fn check_name_deny_list(&self, world_name: &str) -> bool {
        !is_name_banned(&self.banned_names().await.normalized, world_name)
    }

    pub async fn get_banned_names(&self) -> Vec<String> {
        self.banned_names()
            .await
            .raw
            .iter()
            .map(|n| strip_suffixes(n))
            .collect()
    }
}

async fn fetch_banned_names(http: &reqwest::Client, url: Option<&str>) -> Option<Vec<String>> {
    let url = url?;
    let endpoint = format!("{}/banned-names", url);
    match http.post(&endpoint).send().await {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(body) => Some(parse_banned_names(&body)),
            Err(e) => {
                tracing::warn!(error = %e, url = %endpoint, "failed to parse name denylist (keeping last known)");
                None
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, url = %endpoint, "failed to fetch name denylist (keeping last known)");
            None
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

fn is_name_banned(banned: &HashSet<String>, world_name: &str) -> bool {
    banned.contains(&normalize_world_name(world_name))
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
        let banned = BannedNames::from_list(vec!["foo".to_string(), "Evil".to_string()]);
        assert!(is_name_banned(&banned.normalized, "foo.dcl.eth"));
        assert!(is_name_banned(&banned.normalized, "FOO.DCL.ETH"));
        assert!(is_name_banned(&banned.normalized, "evil.dcl.eth"));
        assert!(!is_name_banned(&banned.normalized, "good.dcl.eth"));
    }

    #[test]
    fn empty_banlist_allows_everything() {
        let banned = BannedNames::from_list(Vec::new());
        assert!(!is_name_banned(&banned.normalized, "anything.dcl.eth"));
    }

    #[test]
    fn is_name_banned_hashset_matches_linear_scan_reference() {
        let lists = [
            vec![
                "foo".to_string(),
                "Evil".to_string(),
                "Bar.dcl".to_string(),
                "MiXeD.eth".to_string(),
            ],
            Vec::new(),
        ];
        let names = [
            "foo.dcl.eth",
            "FOO.DCL.ETH",
            "evil.dcl.eth",
            "EVIL",
            "bar.dcl",
            "BAR.DCL.ETH",
            "mixed",
            "MiXeD.dcl.eth",
            "good.dcl.eth",
        ];
        for list in &lists {
            let set = BannedNames::from_list(list.clone());
            for name in &names {
                // The exact expression the code used before the HashSet rewrite.
                let reference = list
                    .iter()
                    .any(|n| n.to_lowercase() == normalize_world_name(name));
                assert_eq!(
                    is_name_banned(&set.normalized, name),
                    reference,
                    "mismatch for list {list:?} name {name:?}"
                );
            }
        }
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
            .insert(
                (),
                Arc::new(BannedNames::from_list(vec!["banned".to_string()])),
            )
            .await;

        assert!(!checker.check_name_deny_list("Banned.dcl.eth").await);
        assert!(checker.check_name_deny_list("fine.dcl.eth").await);
        assert_eq!(checker.get_banned_names().await, vec!["banned".to_string()]);
    }
}
