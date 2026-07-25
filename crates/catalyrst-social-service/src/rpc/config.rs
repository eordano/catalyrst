use anyhow::{Context as _, Result};
use catalyrst_envcfg::{get_int, get_port, get_u64, required};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub auth_window_secs: i64,
    pub database_url: String,
    pub comms_gatekeeper_url: String,
    pub content_database_url: Option<String>,
    pub content_server_address: String,

    pub private_voice_chat_expiration_ms: i64,

    pub private_voice_chat_job_interval_ms: u64,

    pub private_voice_chat_expiration_batch_size: i64,

    pub ws_max_concurrent_connections: Option<usize>,

    pub ws_max_payload_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            http_port: get_port("HTTP_SERVER_PORT", 5148)?,
            auth_window_secs: get_int("AUTH_WINDOW_SECS", 300)?,
            database_url: required("DATABASE_URL")?,
            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5138".into()),
            content_database_url: env::var("CONTENT_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_server_address: env::var("CONTENT_SERVER_ADDRESS")
                .unwrap_or_else(|_| "https://peer.decentraland.org/content".into()),
            private_voice_chat_expiration_ms: get_int("PRIVATE_VOICE_CHAT_EXPIRATION_TIME", 60000)?,
            private_voice_chat_job_interval_ms: get_u64("PRIVATE_VOICE_CHAT_JOB_INTERVAL", 1000)?,
            private_voice_chat_expiration_batch_size: get_int(
                "PRIVATE_VOICE_CHAT_EXPIRATION_BATCH_SIZE",
                20,
            )?,
            ws_max_concurrent_connections: env::var("WS_MAX_CONCURRENT_CONNECTIONS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.parse::<usize>()
                        .context("invalid WS_MAX_CONCURRENT_CONNECTIONS")
                })
                .transpose()?,
            ws_max_payload_bytes: get_u64("WS_MAX_PAYLOAD_LENGTH", 1024 * 1024)? as usize,
        })
    }
}
