use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub places_events_database_url: String,

    /// Bearer token gating the admin moderation routes
    /// (`POST /api/events`, `PATCH /api/events/{id}`). When unset, every admin
    /// route fails closed (403). See docs/admin-console.md §4 (catalyrst-events).
    pub admin_token: Option<String>,

    /// Per-catalyst content-addressed image cache backing the poster upload
    /// routes (replaces the upstream S3 bucket — events.md §3).
    pub content_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5135)?,

            places_events_database_url: required("PLACES_EVENTS_PG_CONNECTION_STRING")?,

            admin_token: env::var("CATALYRST_EVENTS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),

            content_dir: env::var("CATALYRST_EVENTS_CONTENT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/tmp/catalyrst-events-content")),
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
