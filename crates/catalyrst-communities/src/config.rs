use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub admin_token: Option<String>,
    pub communities_content_dir: PathBuf,
    pub content_database_url: Option<String>,
    pub mutes_database_url: Option<String>,
    pub content_server_address: String,
    pub cdn_url: String,
    pub global_moderators: Vec<String>,
    /// Base URL of the places API (`POST /api/destinations`) used to verify that
    /// a community owner actually owns the places they associate. Maps to
    /// upstream `PLACES_API_URL`.
    pub places_api_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let content_dir = env::var("COMMUNITIES_CONTENT_DIR")
            .unwrap_or_else(|_| "./data/communities/content".to_string());
        let global_moderators = env::var("COMMUNITIES_GLOBAL_MODERATORS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 8080)?,
            database_url: required("COMMUNITIES_PG_CONNECTION_STRING")?,
            admin_token: env::var("API_ADMIN_TOKEN").ok().filter(|s| !s.is_empty()),
            communities_content_dir: PathBuf::from(content_dir),
            mutes_database_url: env::var("MUTES_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_database_url: env::var("CONTENT_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_server_address: env::var("CONTENT_SERVER_ADDRESS")
                .unwrap_or_else(|_| "https://peer.decentraland.org/content/".to_string()),
            cdn_url: env::var("CDN_URL")
                .unwrap_or_else(|_| "https://cdn.decentraland.org".to_string()),
            global_moderators,
            places_api_url: env::var("PLACES_API_URL").ok().filter(|s| !s.is_empty()),
        })
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
