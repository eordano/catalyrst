use anyhow::Result;
use catalyrst_envcfg::{get_port, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub badges_database_url: String,

    pub admin_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5147)?,
            badges_database_url: required("BADGES_PG_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_BADGES_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}
