use anyhow::{anyhow, Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub auth_window_secs: i64,
    pub database_url: String,
    pub comms_gatekeeper_url: String,
    pub content_database_url: Option<String>,
    pub content_server_address: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            http_port: env::var("HTTP_SERVER_PORT")
                .unwrap_or_else(|_| "5148".into())
                .parse()
                .context("HTTP_SERVER_PORT must be u16")?,
            auth_window_secs: env::var("AUTH_WINDOW_SECS")
                .unwrap_or_else(|_| "300".into())
                .parse()
                .context("AUTH_WINDOW_SECS must be i64")?,
            database_url: required("DATABASE_URL")?,
            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5138".into()),
            content_database_url: env::var("CONTENT_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_server_address: env::var("CONTENT_SERVER_ADDRESS")
                .unwrap_or_else(|_| "https://peer.decentraland.org/content".into()),
        })
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("missing required env var: {}", key))
}
