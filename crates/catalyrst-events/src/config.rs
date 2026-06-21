use anyhow::Result;
use catalyrst_envcfg::{get_port, required};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub places_events_database_url: String,

    pub admin_token: Option<String>,

    pub content_dir: PathBuf,

    pub comms_gatekeeper_url: String,
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

            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:5138".to_string()),
        })
    }
}
