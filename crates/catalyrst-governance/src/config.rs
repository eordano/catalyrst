use anyhow::{Context, Result};
use catalyrst_envcfg::{get_port, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    pub api_url: String,

    pub poll_enabled: bool,

    pub sync_window_hours: u32,
}

pub const DEFAULT_API_URL: &str = "https://governance.decentraland.org/api";
pub const DEFAULT_SYNC_WINDOW_HOURS: u32 = 48;

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5151)?,
            database_url: required("GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            api_url: env::var("GOVERNANCE_API_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_API_URL.to_string())
                .trim_end_matches('/')
                .to_string(),
            poll_enabled: parse_bool_env("GOVERNANCE_POLL_ENABLED", false),
            sync_window_hours: get_u32("GOVERNANCE_SYNC_WINDOW_HOURS", DEFAULT_SYNC_WINDOW_HOURS)?,
        })
    }
}

fn get_u32(key: &str, default: u32) -> Result<u32> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => s.parse::<u32>().with_context(|| format!("invalid {}", key)),
        _ => Ok(default),
    }
}

pub fn parse_bool_env(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        Err(_) => default,
    }
}
