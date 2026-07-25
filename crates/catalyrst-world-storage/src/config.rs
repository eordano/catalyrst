use anyhow::{anyhow, Result};
use catalyrst_envcfg::{env_bool, get_int, get_port, get_u64, required};
use std::env;

#[derive(Debug, Clone, Copy)]
pub struct NamespaceLimits {
    pub max_value_size_bytes: i64,
    pub max_total_size_bytes: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageCacheConfig {
    pub enabled: bool,
    pub ttl_seconds: u64,
    pub max_entries: u64,
    pub max_value_bytes: usize,
}

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub cors_allowed_origin_suffixes: Vec<String>,

    pub encryption_key: [u8; 32],

    pub authoritative_server_address: Option<String>,

    pub authorized_addresses: Vec<String>,

    pub eip1654_rpc_url: Option<String>,

    pub worlds_content_server_url: String,
    pub lambdas_url: String,
    pub places_url: String,
    pub places_cache_ttl_seconds: u64,
    pub world_permission_cache_ttl_seconds: u64,
    pub storage_cache: StorageCacheConfig,

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
            cors_allowed_origin_suffixes: crate::cors::parse_origin_suffixes(
                &env::var("CORS_ALLOWED_ORIGIN_SUFFIXES")
                    .unwrap_or_else(|_| crate::cors::DEFAULT_ORIGIN_SUFFIXES.to_string()),
            ),
            encryption_key,
            authoritative_server_address,
            authorized_addresses,
            eip1654_rpc_url: env::var("RPC_ENDPOINT_ETH")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| Some("https://rpc.decentraland.org/mainnet".to_string())),
            worlds_content_server_url: env::var("WORLDS_CONTENT_SERVER_URL")
                .unwrap_or_else(|_| "https://worlds-content-server.decentraland.org".to_string()),
            lambdas_url: env::var("LAMBDAS_URL")
                .unwrap_or_else(|_| "https://peer.decentraland.org/lambdas".to_string()),
            places_url: env::var("PLACES_URL")
                .unwrap_or_else(|_| "https://places.decentraland.org".to_string()),
            places_cache_ttl_seconds: get_u64("PLACES_CACHE_TTL_SECONDS", 300)?,
            world_permission_cache_ttl_seconds: get_u64("WORLD_PERMISSIONS_CACHE_TTL_SECONDS", 30)?,
            storage_cache: StorageCacheConfig {
                enabled: env_bool("STORAGE_CACHE_ENABLED", true),
                ttl_seconds: get_u64("STORAGE_CACHE_TTL_SECONDS", 60)?,
                max_entries: get_u64("STORAGE_CACHE_MAX", 8_000)?,
                max_value_bytes: get_u64("STORAGE_CACHE_MAX_VALUE_BYTES", 32_768)? as usize,
            },
            env_limits: NamespaceLimits {
                max_value_size_bytes: get_int("ENV_STORAGE_MAX_VALUE_SIZE_BYTES", 10_240)?,
                max_total_size_bytes: get_int("ENV_STORAGE_MAX_TOTAL_SIZE_BYTES", 262_144)?,
            },
            world_limits: NamespaceLimits {
                max_value_size_bytes: get_int("WORLD_STORAGE_MAX_VALUE_SIZE_BYTES", 524_288)?,
                max_total_size_bytes: get_int("WORLD_STORAGE_MAX_TOTAL_SIZE_BYTES", 10_485_760)?,
            },
            player_limits: NamespaceLimits {
                max_value_size_bytes: get_int("PLAYER_STORAGE_MAX_VALUE_SIZE_BYTES", 102_400)?,
                max_total_size_bytes: get_int("PLAYER_STORAGE_MAX_TOTAL_SIZE_BYTES", 1_048_576)?,
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
