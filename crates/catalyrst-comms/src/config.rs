use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub livekit_host: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_webhook_key: Option<String>,
    pub livekit_configured: bool,
    pub private_messages_room_id: String,
    pub places_api_url: String,
    pub catalyst_url: String,
    /// worlds-content-server base URL — resolves world deployment/streaming ACL
    /// addresses + owner for the /scene-admin extra-address set.
    pub world_content_url: String,
    /// lambdas base URL — resolves Genesis City LAND operators (owner / operator /
    /// updateOperator / updateManagers / approvedForAll) for the extra-address set.
    pub lambdas_url: String,
    pub dapps_database_url: Option<String>,
    pub dapps_schema: String,
    /// places_events archive (resolves place_id -> parcels/world for scene authz).
    pub places_database_url: Option<String>,
    pub authoritative_server_address: Option<String>,
    pub moderator_token: Option<String>,
    pub moderator_addresses: Vec<String>,
    /// Bearer token gating the social-service voice / private-message routes.
    /// Upstream comms-gatekeeper requires `COMMS_GATEKEEPER_AUTH_TOKEN`; here it
    /// is OPT-IN — when unset, the gate is disabled (loopback dev keeps working)
    /// and a warning is logged. Set it before exposing the service off loopback.
    pub gatekeeper_auth_token: Option<String>,
}

fn parse_moderator_addresses(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\n'])
        .map(|s| s.trim().to_lowercase())
        .filter(|a| {
            a.len() == 42 && a.starts_with("0x") && a[2..].chars().all(|c| c.is_ascii_hexdigit())
        })
        .collect()
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let livekit_api_key = env::var("LIVEKIT_API_KEY").unwrap_or_default();
        let livekit_api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_default();
        let livekit_configured = !livekit_api_key.is_empty() && !livekit_api_secret.is_empty();
        let (livekit_api_key, livekit_api_secret) = if livekit_configured {
            (livekit_api_key, livekit_api_secret)
        } else {
            tracing::warn!(
                "LIVEKIT_API_KEY / LIVEKIT_API_SECRET not set; defaulting to devkey/devsecret — \
                 tokens will parse locally but will NOT be accepted by a real LiveKit cluster"
            );
            ("devkey".to_string(), "devsecret".to_string())
        };

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5138)?,
            database_url: required("COMMS_PG_CONNECTION_STRING")?,
            livekit_host: env::var("LIVEKIT_HOST").unwrap_or_else(|_| "livekit.local".to_string()),
            livekit_api_key,
            livekit_api_secret,
            livekit_webhook_key: env::var("LIVEKIT_WEBHOOK_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            livekit_configured,
            private_messages_room_id: env::var("PRIVATE_MESSAGES_ROOM_ID")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "private-messages".to_string()),
            places_api_url: env::var("PLACES_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5134".to_string()),
            catalyst_url: env::var("CATALYST_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5140".to_string()),
            world_content_url: env::var("WORLD_CONTENT_URL")
                .unwrap_or_else(|_| "https://worlds-content-server.decentraland.org".to_string())
                .trim_end_matches('/')
                .to_string(),
            lambdas_url: env::var("LAMBDAS_URL")
                .unwrap_or_else(|_| "https://peer.decentraland.org/lambdas".to_string()),
            dapps_database_url: env::var("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            dapps_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),
            places_database_url: env::var("PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            authoritative_server_address: env::var("AUTHORITATIVE_SERVER_ADDRESS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
            moderator_token: env::var("MODERATOR_TOKEN").ok().filter(|s| !s.is_empty()),
            moderator_addresses: env::var("PLATFORM_USER_MODERATORS")
                .ok()
                .map(|s| parse_moderator_addresses(&s))
                .unwrap_or_default(),
            gatekeeper_auth_token: env::var("COMMS_GATEKEEPER_AUTH_TOKEN")
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
