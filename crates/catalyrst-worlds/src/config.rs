use anyhow::{anyhow, Result};
use catalyrst_envcfg::{env_bool, get_int, get_port, get_u64, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub http_base_url: String,
    pub network_id: i64,

    pub squid_database_url: Option<String>,
    pub global_scenes_urn: Option<String>,
    pub content_public_url: String,
    pub lambdas_public_url: String,

    pub livekit_host: String,
    pub livekit_ws_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_configured: bool,
    pub livekit_webhook_key: Option<String>,
    pub max_users_per_world: i64,

    pub contents_upstream_url: String,
    pub contents_dir: std::path::PathBuf,

    pub comms_gatekeeper_url: Option<String>,
    pub comms_gatekeeper_auth_token: Option<String>,

    pub denylist_json_url: Option<String>,

    pub dcl_lists_url: Option<String>,

    pub admin_token: Option<String>,

    pub max_in_flight_upload_bytes: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let http_port = get_port("HTTP_SERVER_PORT", 5146)?;

        let livekit_api_key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
        let livekit_api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_default();
        let livekit_configured = !livekit_api_key.is_empty() && !livekit_api_secret.is_empty();
        let (livekit_api_key, livekit_api_secret) = if livekit_configured {
            (livekit_api_key, livekit_api_secret)
        } else if env_bool("LIVEKIT_ALLOW_DEV_CREDS", false) {
            tracing::warn!(
                "LIVEKIT_API_KEY / LIVEKIT_API_SECRET not set; defaulting to devkey/devsecret — \
                 tokens will parse locally but will NOT be accepted by a real LiveKit cluster"
            );
            ("devkey".to_string(), "devsecret".to_string())
        } else {
            return Err(anyhow!(
                "LIVEKIT_API_KEY / LIVEKIT_API_SECRET are not set; set both, or set \
                 LIVEKIT_ALLOW_DEV_CREDS=1 to run with the devkey/devsecret dev defaults"
            ));
        };

        let http_base_url = env::var("HTTP_BASE_URL")
            .unwrap_or_else(|_| format!("http://127.0.0.1:{}", http_port))
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port,
            database_url: required("WORLDS_PG_CONNECTION_STRING")?,
            http_base_url,
            network_id: get_int("NETWORK_ID", 1)?,
            squid_database_url: env::var("SQUID_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            global_scenes_urn: env::var("GLOBAL_SCENES_URN").ok().filter(|s| !s.is_empty()),
            content_public_url: env::var("CONTENT_PUBLIC_URL")
                .unwrap_or_else(|_| "https://peer.decentraland.org/content".to_string()),
            lambdas_public_url: env::var("LAMBDAS_PUBLIC_URL")
                .unwrap_or_else(|_| "https://peer.decentraland.org/lambdas".to_string()),
            livekit_host: env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string()),
            livekit_ws_url: env::var("LIVEKIT_WS_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "wss://{}",
                        env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string())
                    )
                }),
            livekit_api_key,
            livekit_api_secret,
            livekit_configured,
            livekit_webhook_key: env::var("LIVEKIT_WEBHOOK_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            max_users_per_world: get_int("MAX_USERS_PER_WORLD", 100)?,
            contents_dir: std::path::PathBuf::from(
                env::var("WORLDS_CONTENT_DIR")
                    .unwrap_or_else(|_| "./data/worlds/contents".to_string()),
            ),
            contents_upstream_url: env::var("CONTENTS_UPSTREAM_URL")
                .unwrap_or_else(|_| "https://worlds-content-server.decentraland.org".to_string())
                .trim_end_matches('/')
                .to_string(),
            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string()),
            comms_gatekeeper_auth_token: env::var("COMMS_GATEKEEPER_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            denylist_json_url: env::var("DENYLIST_JSON_URL").ok().filter(|s| !s.is_empty()),
            dcl_lists_url: env::var("DCL_LISTS_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string()),
            admin_token: env::var("CATALYRST_WORLDS_ADMIN_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            max_in_flight_upload_bytes: get_u64(
                "MAX_IN_FLIGHT_UPLOAD_BYTES",
                crate::handlers::deploy::DEFAULT_MAX_IN_FLIGHT_UPLOAD_BYTES,
            )?,
        })
    }
}
