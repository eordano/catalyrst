use anyhow::{Context, Result};
use std::env;

pub const DEFAULT_ABGEN_OUT_ROOT: &str = "./data/ab-generator/out";

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub abgen_out_root: String,
    /// Optional live-conversion upstream (`ABGEN_LIVE_UPSTREAM`, e.g.
    /// `http://127.0.0.1:5185`). When set, any request that misses the local
    /// `abgen_out_root` is reverse-proxied to this JIT converter (abgen-serve),
    /// which builds the bundle on demand. Unset ⇒ pure static behavior (404 on
    /// miss). Trailing slash is trimmed.
    pub live_upstream: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5147)?,
            abgen_out_root: env::var("ABGEN_OUT_ROOT")
                .unwrap_or_else(|_| DEFAULT_ABGEN_OUT_ROOT.to_string()),
            live_upstream: env::var("ABGEN_LIVE_UPSTREAM")
                .ok()
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}

fn get_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(s) => s.parse::<u16>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}
