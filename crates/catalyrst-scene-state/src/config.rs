use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub local_scene_path: Option<String>,

    pub world_server_url: Option<String>,

    pub debugging_secret: Option<String>,

    pub admin_token: Option<String>,

    pub http_base_url: Option<String>,

    pub auth_timeout_secs: u64,

    pub disable_js_runtime: bool,

    pub realm_name: Option<String>,

    pub commit_hash: String,

    pub js_heap_limit_mb: usize,

    pub js_tick_budget_ms: u64,

    pub js_shutdown_join_ms: u64,

    pub client_outbound_max: usize,

    pub client_inbound_max: usize,

    pub crdt_max_components: usize,

    pub ws_max_frame_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let http_host = env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let http_port: u16 = env::var("HTTP_SERVER_PORT")
            .unwrap_or_else(|_| "5153".into())
            .parse()
            .context("HTTP_SERVER_PORT must be u16")?;

        let opt = |k: &str| env::var(k).ok().filter(|s| !s.is_empty());

        let debugging_secret = opt("DEBUGGING_SECRET");

        let admin_token =
            opt("CATALYRST_SCENE_STATE_ADMIN_TOKEN").or_else(|| debugging_secret.clone());

        Ok(Self {
            http_host,
            http_port,
            local_scene_path: opt("LOCAL_SCENE_PATH"),
            world_server_url: opt("WORLD_SERVER_URL"),
            debugging_secret,
            admin_token,
            http_base_url: opt("HTTP_BASE_URL"),
            auth_timeout_secs: opt("AUTH_TIMEOUT_SECS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            disable_js_runtime: opt("DISABLE_JS_RUNTIME")
                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            realm_name: opt("REALM_NAME"),
            commit_hash: env::var("COMMIT_HASH").unwrap_or_default(),
            js_heap_limit_mb: parse_or("JS_HEAP_LIMIT_MB", 384),
            js_tick_budget_ms: parse_or("JS_TICK_BUDGET_MS", 250),
            js_shutdown_join_ms: parse_or("JS_SHUTDOWN_JOIN_MS", 2000),
            client_outbound_max: parse_or("CLIENT_OUTBOUND_MAX", 1024),
            client_inbound_max: parse_or("CLIENT_INBOUND_MAX", 1024),
            crdt_max_components: parse_or("CRDT_MAX_COMPONENTS", 100_000),
            ws_max_frame_bytes: parse_or("WS_MAX_FRAME_BYTES", 2 * 1024 * 1024),
        })
    }
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
