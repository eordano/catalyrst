use anyhow::{anyhow, Context, Result};
use std::env;

/// hCaptcha/reCAPTCHA-style siteverify endpoint, used when an external captcha
/// provider gates the claim in addition to the upstream slider puzzle.
const DEFAULT_CAPTCHA_VERIFY_URL: &str = "https://hcaptcha.com/siteverify";

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    /// Bearer token gating the high-risk financial admin routes
    /// (seasons/goals CRUD, grant/revoke credits, block/unblock a user).
    /// When unset, every admin route fails closed (403).
    pub admin_token: Option<String>,
    /// Server-side secret for the external captcha provider. When set, the claim
    /// requires a provider token verified against `captcha_verify_url` in addition
    /// to the slider answer; when unset, the upstream slider gate stands alone.
    pub captcha_secret: Option<String>,
    /// Provider siteverify endpoint (form-POST `secret`+`response`, JSON `success`).
    pub captcha_verify_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5150)?,
            database_url: required("CREDITS_PG_CONNECTION_STRING")?,
            admin_token: env::var("CATALYRST_CREDITS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            captcha_secret: env::var("CREDITS_CAPTCHA_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            captcha_verify_url: env::var("CREDITS_CAPTCHA_VERIFY_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_CAPTCHA_VERIFY_URL.to_string()),
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
