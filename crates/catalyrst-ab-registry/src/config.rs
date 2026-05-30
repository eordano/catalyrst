use anyhow::{Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub content_database_url: String,

    pub ab_registry_database_url: Option<String>,

    pub abgen_out_root: String,

    pub admin_token: Option<String>,

    pub profile_images_url: String,

    pub denylist_moderators: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5143)?,
            content_database_url: content_connection_string()?,
            ab_registry_database_url: env::var("AB_REGISTRY_PG_CONNECTION_STRING").ok(),
            abgen_out_root: env::var("ABGEN_OUT_ROOT")
                .unwrap_or_else(|_| "./data/ab-generator/workdir/out".to_string()),
            admin_token: env::var("API_ADMIN_TOKEN").ok(),
            profile_images_url: env::var("PROFILE_IMAGES_URL")
                .unwrap_or_else(|_| "https://profile-images.decentraland.org".to_string())
                .trim_end_matches('/')
                .to_string(),
            denylist_moderators: env::var("DENYLIST_MODERATORS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        })
    }
}

fn content_connection_string() -> Result<String> {
    if let Ok(url) = env::var("CONTENT_PG_CONNECTION_STRING") {
        return Ok(url);
    }
    let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "./data/run".into());
    let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "6432".into());
    let user = env::var("POSTGRES_CONTENT_USER")
        .context("missing POSTGRES_CONTENT_USER (or CONTENT_PG_CONNECTION_STRING)")?;
    let password = env::var("POSTGRES_CONTENT_PASSWORD").unwrap_or_default();
    let db = env::var("POSTGRES_CONTENT_DB").unwrap_or_else(|_| "content".into());

    let esc = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    Ok(format!(
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
