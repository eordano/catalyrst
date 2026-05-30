use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub content_bucket_url: String,
    pub admin_addresses: Vec<String>,
    pub newsletter_service_url: Option<String>,
    pub newsletter_publication_id: Option<String>,
    pub newsletter_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let admin_addresses = env::var("BUILDER_ADMIN_ADDRESSES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5145)?,
            database_url: required("BUILDER_PG_CONNECTION_STRING")?,
            content_bucket_url: env::var("BUILDER_CONTENT_BUCKET_URL")
                .unwrap_or_else(|_| "https://builder-items.decentraland.org".to_string()),
            admin_addresses,
            newsletter_service_url: env::var("NEWSLETTER_SERVICE_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            newsletter_publication_id: env::var("NEWSLETTER_PUBLICATION_ID")
                .ok()
                .filter(|s| !s.is_empty()),
            newsletter_api_key: env::var("NEWSLETTER_SERVICE_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
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
