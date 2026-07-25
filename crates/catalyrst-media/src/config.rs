use anyhow::{anyhow, Result};
use catalyrst_envcfg::{get_port, required};
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Mock,
    Http,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Mock => "mock",
            BackendKind::Http => "http",
        }
    }
}

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub backend_kind: BackendKind,
    pub backend_url: Option<String>,
    pub backend_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let backend_url = env::var("TRANSLATE_BACKEND_URL")
            .ok()
            .filter(|s| !s.is_empty());
        let backend_kind = match env::var("TRANSLATE_BACKEND").ok().as_deref() {
            Some("mock") => BackendKind::Mock,
            Some("http") => BackendKind::Http,
            _ if backend_url.is_some() => BackendKind::Http,
            _ => BackendKind::Mock,
        };
        if backend_kind == BackendKind::Http && backend_url.is_none() {
            return Err(anyhow!(
                "TRANSLATE_BACKEND=http requires TRANSLATE_BACKEND_URL"
            ));
        }
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5157)?,
            database_url: required("MEDIA_PG_CONNECTION_STRING")?,
            backend_kind,
            backend_url,
            backend_api_key: env::var("TRANSLATE_BACKEND_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}
