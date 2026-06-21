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
    /// Bearer token gating the §4 admin moderation routes. When `None`
    /// (env unset) every admin route fails closed with 403.
    pub admin_token: Option<String>,
    /// Opt-in: when true, `GET /v1/trades` paginates (first/skip + total count)
    /// instead of returning the entire trades table. Default false to preserve
    /// parity with upstream marketplace-server (which returns all trades).
    pub trades_pagination: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
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
            admin_token: env::var("CATALYRST_MARKET_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            trades_pagination: env_bool("CATALYRST_MARKET_TRADES_PAGINATION"),
        })
    }
}

/// Parse a boolean env var. Truthy values: `1`, `true`, `yes`, `on`
/// (case-insensitive). Anything else — including unset — is false.
fn env_bool(key: &str) -> bool {
    matches!(
        env::var(key)
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
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
