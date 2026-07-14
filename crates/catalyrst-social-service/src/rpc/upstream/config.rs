use super::identity::UpstreamIdentity;
use std::env;

pub const RPC_URL_VAR: &str = "UPSTREAM_SOCIAL_RPC_URL";
pub const API_URL_VAR: &str = "UPSTREAM_SOCIAL_API_URL";
pub const KEY_FILE_VAR: &str = "UPSTREAM_SOCIAL_KEY_FILE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpstreamConfig {
    pub social_rpc_url: Option<String>,
    pub social_api_url: Option<String>,
    pub key_file: String,
}

impl UpstreamConfig {
    pub fn from_env() -> Option<Self> {
        Self::from_vars(
            env::var(RPC_URL_VAR).ok(),
            env::var(API_URL_VAR).ok(),
            env::var(KEY_FILE_VAR).ok(),
        )
    }

    pub fn from_vars(
        rpc_url: Option<String>,
        api_url: Option<String>,
        key_file: Option<String>,
    ) -> Option<Self> {
        let non_empty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        let social_rpc_url = non_empty(rpc_url);
        let social_api_url = non_empty(api_url);
        let key_file = non_empty(key_file)?;
        if social_rpc_url.is_none() && social_api_url.is_none() {
            return None;
        }
        Some(Self {
            social_rpc_url,
            social_api_url,
            key_file,
        })
    }

    pub fn identity(&self) -> anyhow::Result<UpstreamIdentity> {
        UpstreamIdentity::from_key_file(&self.key_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn absent_key_file_disables_the_bridge() {
        assert_eq!(
            UpstreamConfig::from_vars(s("wss://up"), s("https://up"), None),
            None
        );
        assert_eq!(
            UpstreamConfig::from_vars(s("wss://up"), None, s("  ")),
            None
        );
    }

    #[test]
    fn absent_urls_disable_the_bridge() {
        assert_eq!(UpstreamConfig::from_vars(None, None, s("/k")), None);
        assert_eq!(UpstreamConfig::from_vars(s(""), s(" "), s("/k")), None);
    }

    #[test]
    fn either_url_with_a_key_enables_the_bridge() {
        let rpc_only = UpstreamConfig::from_vars(s("wss://up"), None, s("/k")).unwrap();
        assert_eq!(rpc_only.social_rpc_url.as_deref(), Some("wss://up"));
        assert_eq!(rpc_only.social_api_url, None);
        let api_only = UpstreamConfig::from_vars(None, s("https://up"), s("/k")).unwrap();
        assert_eq!(api_only.social_api_url.as_deref(), Some("https://up"));
        assert_eq!(api_only.social_rpc_url, None);
        let both = UpstreamConfig::from_vars(s("wss://up"), s("https://up"), s("/k")).unwrap();
        assert_eq!(both.key_file, "/k");
    }

    #[test]
    fn from_env_mirrors_the_variable_presence() {
        if env::var(super::super::session::LIVE_TEST_VAR).is_ok() {
            return;
        }
        env::remove_var(RPC_URL_VAR);
        env::remove_var(API_URL_VAR);
        env::remove_var(KEY_FILE_VAR);
        assert_eq!(UpstreamConfig::from_env(), None);
        env::set_var(RPC_URL_VAR, "wss://upstream.example");
        env::set_var(KEY_FILE_VAR, "/tmp/k");
        let cfg = UpstreamConfig::from_env().unwrap();
        assert_eq!(
            cfg.social_rpc_url.as_deref(),
            Some("wss://upstream.example")
        );
        assert_eq!(cfg.social_api_url, None);
        assert_eq!(cfg.key_file, "/tmp/k");
        env::remove_var(RPC_URL_VAR);
        env::remove_var(KEY_FILE_VAR);
        assert_eq!(UpstreamConfig::from_env(), None);
    }

    #[test]
    fn identity_surfaces_a_missing_key_file_as_an_error() {
        let cfg =
            UpstreamConfig::from_vars(s("wss://up"), None, s("/nonexistent/upstream.key")).unwrap();
        assert!(cfg.identity().is_err());
    }
}
