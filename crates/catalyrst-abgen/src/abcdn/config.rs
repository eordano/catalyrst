use anyhow::{Context, Result};
use std::env;

pub const DEFAULT_ABGEN_OUT_ROOT: &str = "./data/ab-generator/out";

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub abgen_out_root: String,

    pub content_url: String,

    pub content_disk: Option<String>,

    pub live_cache_dir: String,

    pub live_version: String,

    pub manifest_content_server_url: String,

    pub abgen_root: Option<String>,

    pub content_database_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5147)?,
            abgen_out_root: env::var("ABGEN_OUT_ROOT")
                .unwrap_or_else(|_| DEFAULT_ABGEN_OUT_ROOT.to_string()),
            content_url: env::var("ABGEN_CATALYST_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:5141/content".to_string()),
            content_disk: env::var("ABGEN_CONTENT_DISK")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            live_cache_dir: env::var("ABGEN_CACHE_DIR")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "./abgen-serve-cache".to_string()),
            live_version: env::var("ABGEN_VERSION")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "v41".to_string()),
            manifest_content_server_url: env::var("ABGEN_MANIFEST_CONTENT_SERVER_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| crate::manifest::DEFAULT_CONTENT_SERVER_URL.to_string()),
            abgen_root: env::var("ABGEN_ROOT")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            content_database_url: content_connection_string(),
        })
    }
}

fn content_connection_string() -> Option<String> {
    if let Ok(url) = env::var("CONTENT_PG_CONNECTION_STRING") {
        if !url.trim().is_empty() {
            return Some(url);
        }
    }
    let user = env::var("POSTGRES_CONTENT_USER")
        .ok()
        .filter(|s| !s.is_empty())?;
    let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "./data/run".into());
    let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "6432".into());
    let password = env::var("POSTGRES_CONTENT_PASSWORD").unwrap_or_default();
    let db = env::var("POSTGRES_CONTENT_DB").unwrap_or_else(|_| "content".into());
    let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    Some(format!(
        "host='{}' port={} user='{}' password='{}' dbname='{}' connect_timeout=30",
        esc(&host),
        port,
        esc(&user),
        esc(&password),
        esc(&db),
    ))
}

fn get_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(s) => s.parse::<u16>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}
