use anyhow::{anyhow, Result};
use catalyrst_envcfg::{get_port, get_u64, required};
use std::env;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Mock,
    Http,
    Llm,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Mock => "mock",
            BackendKind::Http => "http",
            BackendKind::Llm => "llm",
        }
    }
}

pub const DEFAULT_LLM_MODEL: &str = "gpt-4o-mini";

// Abuse bounds for the unauthenticated public /translate. Char limit is
// LibreTranslate parity: characters are summed across the whole batch, not
// per item, so a batch cannot multiply the per-request work.
pub const DEFAULT_TRANSLATE_CHAR_LIMIT: u64 = 5000;
pub const DEFAULT_TRANSLATE_BATCH_LIMIT: u64 = 100;
pub const DEFAULT_TRANSLATE_REQUEST_TIMEOUT_SECS: u64 = 30;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub backend_kind: BackendKind,
    pub backend_url: Option<String>,
    pub backend_api_key: Option<String>,
    pub llm_base_url: Option<String>,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub translate_char_limit: usize,
    pub translate_batch_limit: usize,
    pub translate_request_timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let backend_url = env::var("TRANSLATE_BACKEND_URL")
            .ok()
            .filter(|s| !s.is_empty());
        let llm_base_url = env::var("TRANSLATE_LLM_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());
        // Selection is fail-closed: with TRANSLATE_BACKEND unset we stay on the mock and
        // never emit a real LLM/upstream call. `llm` and `http` demand their own explicit URL.
        let backend_kind = match env::var("TRANSLATE_BACKEND").ok().as_deref() {
            Some("mock") => BackendKind::Mock,
            Some("http") => BackendKind::Http,
            Some("llm") => BackendKind::Llm,
            _ if backend_url.is_some() => BackendKind::Http,
            _ => BackendKind::Mock,
        };
        if backend_kind == BackendKind::Http && backend_url.is_none() {
            return Err(anyhow!(
                "TRANSLATE_BACKEND=http requires TRANSLATE_BACKEND_URL"
            ));
        }
        if backend_kind == BackendKind::Llm && llm_base_url.is_none() {
            return Err(anyhow!(
                "TRANSLATE_BACKEND=llm requires TRANSLATE_LLM_BASE_URL (e.g. https://llm.decent.dev)"
            ));
        }
        let translate_char_limit =
            get_u64("TRANSLATE_CHAR_LIMIT", DEFAULT_TRANSLATE_CHAR_LIMIT)? as usize;
        if translate_char_limit == 0 {
            return Err(anyhow!("TRANSLATE_CHAR_LIMIT must be >= 1"));
        }
        let translate_batch_limit =
            get_u64("TRANSLATE_BATCH_LIMIT", DEFAULT_TRANSLATE_BATCH_LIMIT)? as usize;
        if translate_batch_limit == 0 {
            return Err(anyhow!("TRANSLATE_BATCH_LIMIT must be >= 1"));
        }
        let translate_request_timeout_secs = get_u64(
            "TRANSLATE_REQUEST_TIMEOUT_SECS",
            DEFAULT_TRANSLATE_REQUEST_TIMEOUT_SECS,
        )?;
        if translate_request_timeout_secs == 0 {
            return Err(anyhow!("TRANSLATE_REQUEST_TIMEOUT_SECS must be >= 1"));
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
            llm_base_url,
            llm_api_key: env::var("TRANSLATE_LLM_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            llm_model: env::var("TRANSLATE_LLM_MODEL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string()),
            translate_char_limit,
            translate_batch_limit,
            translate_request_timeout: Duration::from_secs(translate_request_timeout_secs),
        })
    }
}
