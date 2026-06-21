use anyhow::{anyhow, Context, Result};
use std::env;

/// Per-namespace byte-size limits, mirroring the upstream
/// `*_STORAGE_MAX_VALUE_SIZE_BYTES` / `*_STORAGE_MAX_TOTAL_SIZE_BYTES` knobs.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceLimits {
    pub max_value_size_bytes: i64,
    pub max_total_size_bytes: i64,
}

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,

    /// 32-byte AES-256-GCM key, hex-encoded (64 hex chars).
    pub encryption_key: [u8; 32],

    /// Lowercased authoritative-server address (env-var read access).
    pub authoritative_server_address: Option<String>,
    /// Lowercased extra authorized addresses (comma-separated upstream).
    pub authorized_addresses: Vec<String>,

    pub worlds_content_server_url: String,
    pub lambdas_url: String,
    pub places_url: String,
    pub places_cache_ttl_seconds: u64,

    pub env_limits: NamespaceLimits,
    pub world_limits: NamespaceLimits,
    pub player_limits: NamespaceLimits,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let key_hex = required("ENCRYPTION_KEY")?;
        let encryption_key = parse_encryption_key(&key_hex)?;

        let authoritative_server_address = env::var("AUTHORITATIVE_SERVER_ADDRESS")
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());

        let authorized_addresses = env::var("AUTHORIZED_ADDRESSES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5151)?,
            database_url: required("WORLD_STORAGE_PG_CONNECTION_STRING")?,
            encryption_key,
            authoritative_server_address,
            authorized_addresses,
            worlds_content_server_url: env::var("WORLDS_CONTENT_SERVER_URL")
                .unwrap_or_else(|_| "https://worlds-content-server.decentraland.org".to_string()),
            lambdas_url: env::var("LAMBDAS_URL")
                .unwrap_or_else(|_| "https://peer.decentraland.org/lambdas".to_string()),
            places_url: env::var("PLACES_URL")
                .unwrap_or_else(|_| "https://places.decentraland.org".to_string()),
            places_cache_ttl_seconds: get_u64("PLACES_CACHE_TTL_SECONDS", 300)?,
            env_limits: NamespaceLimits {
                max_value_size_bytes: get_i64("ENV_STORAGE_MAX_VALUE_SIZE_BYTES", 10_240)?,
                max_total_size_bytes: get_i64("ENV_STORAGE_MAX_TOTAL_SIZE_BYTES", 262_144)?,
            },
            world_limits: NamespaceLimits {
                max_value_size_bytes: get_i64("WORLD_STORAGE_MAX_VALUE_SIZE_BYTES", 524_288)?,
                max_total_size_bytes: get_i64("WORLD_STORAGE_MAX_TOTAL_SIZE_BYTES", 10_485_760)?,
            },
            player_limits: NamespaceLimits {
                max_value_size_bytes: get_i64("PLAYER_STORAGE_MAX_VALUE_SIZE_BYTES", 102_400)?,
                max_total_size_bytes: get_i64("PLAYER_STORAGE_MAX_TOTAL_SIZE_BYTES", 1_048_576)?,
            },
        })
    }
}

fn parse_encryption_key(key_hex: &str) -> Result<[u8; 32]> {
    if key_hex.len() != 64 {
        return Err(anyhow!(
            "invalid ENCRYPTION_KEY length: expected 64 hex characters, got {}",
            key_hex.len()
        ));
    }
    let bytes = hex::decode(key_hex)
        .map_err(|_| anyhow!("invalid ENCRYPTION_KEY: contains non-hexadecimal characters"))?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
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

fn get_i64(key: &str, default: i64) -> Result<i64> {
    match env::var(key) {
        Ok(s) => s.parse::<i64>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

fn get_u64(key: &str, default: u64) -> Result<u64> {
    match env::var(key) {
        Ok(s) => s.parse::<u64>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}
