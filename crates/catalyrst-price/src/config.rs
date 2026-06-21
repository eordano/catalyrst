use anyhow::Result;
use catalyrst_envcfg::{env_bool, get_port, get_u64, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub price_database_url: String,

    pub admin_token: Option<String>,

    pub price_poll_enabled: bool,

    pub coingecko_url: String,

    pub price_poll_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5156)?,
            price_database_url: required("PRICE_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_PRICE_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            price_poll_enabled: env_bool("PRICE_POLL_ENABLED", false),
            coingecko_url: env::var("COINGECKO_URL")
                .ok()
                .map(|s| s.trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.coingecko.com/api/v3".to_string()),
            price_poll_interval_secs: get_u64("PRICE_POLL_INTERVAL_SECS", 300)?,
        })
    }
}
