use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    pub content_storage_dir: String,

    pub api_url: String,

    pub bucket_url: Option<String>,

    pub max_images_per_user: u64,

    pub places_api_url: String,
    pub places_cache_ttl_seconds: u64,
    pub places_cache_max_size: u64,

    /// Bearer token gating the moderator `/admin/*` routes. When unset, those
    /// routes fail closed (403). Compared in constant time.
    pub admin_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5149)?,
            database_url: required("CAMERA_REEL_PG_CONNECTION_STRING")?,
            content_storage_dir: env::var("CONTENT_STORAGE_DIR")
                .unwrap_or_else(|_| "./data/camera-reel".to_string()),
            api_url: env::var("API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5149".to_string()),
            bucket_url: env::var("BUCKET_URL").ok().filter(|s| !s.is_empty()),
            max_images_per_user: get_u64("MAX_IMAGES_PER_USER", 500)?,
            places_api_url: env::var("PLACES_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5134".to_string()),
            places_cache_ttl_seconds: get_u64("PLACES_CACHE_TTL_SECONDS", 300)?,
            places_cache_max_size: get_u64("PLACES_CACHE_MAX_SIZE", 1000)?,
            admin_token: env::var("CATALYRST_CAMERA_REEL_ADMIN_TOKEN")
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

fn get_u64(key: &str, default: u64) -> Result<u64> {
    match env::var(key) {
        Ok(s) => s.parse::<u64>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}
