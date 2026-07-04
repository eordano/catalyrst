use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub dapps_database_url: String,
    pub dapps_schema: String,
    pub dapps_read_database_url: String,
    pub dapps_read_schema: String,
    pub favorites_database_url: String,
    pub favorites_schema: String,
    pub content_database_url: Option<String>,

    pub admin_token: Option<String>,

    pub trades_pagination: bool,

    /// Upstream signed-trade book to poll (empty env value disables the sync).
    pub trades_sync_upstream_url: Option<String>,

    pub trades_sync_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let cfg = Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5133)?,
            dapps_database_url: required("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            dapps_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "marketplace".to_string()),
            dapps_read_database_url: required("DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            dapps_read_schema: env::var("DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "marketplace".to_string()),
            favorites_database_url: required("FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            favorites_schema: env::var("FAVORITES_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "favorites".to_string()),
            content_database_url: env::var("CONTENT_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            admin_token: env::var("CATALYRST_MARKET_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            trades_pagination: env_bool_default("CATALYRST_MARKET_TRADES_PAGINATION", true),
            trades_sync_upstream_url: match env::var("TRADES_SYNC_UPSTREAM_URL") {
                Ok(v) if v.trim().is_empty() => None,
                Ok(v) => Some(v.trim().to_string()),
                Err(_) => Some(crate::trades_sync::DEFAULT_TRADES_SYNC_UPSTREAM_URL.to_string()),
            },
            trades_sync_interval_secs: match env::var("TRADES_SYNC_INTERVAL_SECS") {
                Ok(v) => v
                    .trim()
                    .parse::<u64>()
                    .with_context(|| format!("invalid TRADES_SYNC_INTERVAL_SECS: {v:?}"))?,
                Err(_) => crate::trades_sync::DEFAULT_TRADES_SYNC_INTERVAL_SECS,
            },
        };
        guard_admin_exposure(
            &cfg.http_host,
            cfg.admin_token.as_deref(),
            "CATALYRST_MARKET_ADMIN_TOKEN",
        )?;
        Ok(cfg)
    }
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let h = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(h);
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

fn guard_admin_exposure(host: &str, admin_token: Option<&str>, token_env: &str) -> Result<()> {
    if is_loopback_host(host) {
        return Ok(());
    }
    if admin_token.is_none() {
        return Err(anyhow!(
            "refusing to start: HTTP_SERVER_HOST={host:?} is not a loopback address, which exposes \
             the loopback-only admin endpoints to the network, and no {token_env} is set to guard \
             them. Bind 127.0.0.1 (front the public API with nginx) or set {token_env}."
        ));
    }
    tracing::warn!(
        host = %host,
        "HTTP_SERVER_HOST is non-loopback: the admin surface is reachable from the network and \
         protected only by the bearer token. Prefer binding 127.0.0.1 behind nginx."
    );
    Ok(())
}

#[allow(dead_code)]
fn env_bool(key: &str) -> bool {
    env_bool_default(key, false)
}

fn env_bool_default(key: &str, default: bool) -> bool {
    match env::var(key)
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        _ => default,
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("missing required env var: {}", key))
}

fn get_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(s) => s.parse::<u16>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod exposure_tests {
    use super::{guard_admin_exposure, is_loopback_host};

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("localhost"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("10.0.0.5"));
    }

    #[test]
    fn guard_policy() {
        assert!(guard_admin_exposure("127.0.0.1", None, "T").is_ok());
        assert!(guard_admin_exposure("0.0.0.0", None, "T").is_err());
        assert!(guard_admin_exposure("0.0.0.0", Some("tok"), "T").is_ok());
    }
}
