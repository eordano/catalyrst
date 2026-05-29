use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::entity::EntityType;

pub const DEFAULT_STORAGE_ROOT_FOLDER: &str = "storage";
pub const DEFAULT_HTTP_SERVER_PORT: u16 = 6969;
pub const DEFAULT_HTTP_SERVER_HOST: &str = "0.0.0.0";
pub const DEFAULT_DENYLIST_FILE_NAME: &str = "denylist.txt";
pub const DEFAULT_DENYLIST_URLS: &str = "https://asset-bundle-registry.decentraland.org/denylist";
pub const DECENTRALAND_ADDRESS: &str = "0x1337e0507eb4ab47e08a179573ed4533d9e22a7b";
pub const DEFAULT_FOLDER_MIGRATION_MAX_CONCURRENCY: u32 = 1000;
pub const DEFAULT_ENTITIES_CACHE_SIZE: u32 = 150_000;
pub const DEFAULT_ETH_NETWORK: &str = "sepolia";

pub const DEFAULT_ENS_OWNER_PROVIDER_URL_TESTNET: &str =
    "https://api.studio.thegraph.com/query/49472/marketplace-sepolia/version/latest";
pub const DEFAULT_ENS_OWNER_PROVIDER_URL_MAINNET: &str =
    "https://subgraph.decentraland.org/marketplace";
pub const DEFAULT_LAND_MANAGER_SUBGRAPH_TESTNET: &str =
    "https://api.studio.thegraph.com/query/49472/land-manager-sepolia/version/latest";
pub const DEFAULT_LAND_MANAGER_SUBGRAPH_MAINNET: &str =
    "https://subgraph.decentraland.org/land-manager";
pub const DEFAULT_COLLECTIONS_SUBGRAPH_TESTNET: &str =
    "https://api.studio.thegraph.com/query/49472/collections-ethereum-sepolia/version/latest";
pub const DEFAULT_COLLECTIONS_SUBGRAPH_MAINNET: &str =
    "https://subgraph.decentraland.org/collections-ethereum-mainnet";
pub const DEFAULT_COLLECTIONS_SUBGRAPH_MATIC_MAINNET: &str =
    "https://subgraph.decentraland.org/collections-matic-mainnet";
pub const DEFAULT_COLLECTIONS_SUBGRAPH_MATIC_AMOY: &str =
    "https://subgraph.decentraland.org/collections-matic-amoy";
pub const DEFAULT_THIRD_PARTY_REGISTRY_SUBGRAPH_MATIC_AMOY: &str =
    "https://subgraph.decentraland.org/tpr-matic-amoy";
pub const DEFAULT_THIRD_PARTY_REGISTRY_SUBGRAPH_MATIC_MAINNET: &str =
    "https://subgraph.decentraland.org/tpr-matic-mainnet";
pub const DEFAULT_BLOCKS_SUBGRAPH_TESTNET: &str =
    "https://api.studio.thegraph.com/query/49472/blocks-ethereum-sepolia/version/latest";
pub const DEFAULT_BLOCKS_SUBGRAPH_MAINNET: &str =
    "https://subgraph.decentraland.org/blocks-ethereum-mainnet";
pub const DEFAULT_BLOCKS_SUBGRAPH_MATIC_AMOY: &str =
    "https://api.studio.thegraph.com/query/49472/blocks-matic-amoy/version/latest";
pub const DEFAULT_BLOCKS_SUBGRAPH_MATIC_MAINNET: &str =
    "https://subgraph.decentraland.org/blocks-matic-mainnet";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub password: String,
    pub user: String,
    pub database: String,
    pub host: String,
    pub schema: String,
    pub port: u16,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            password: "12345678".into(),
            user: "postgres".into(),
            database: "content".into(),
            host: "localhost".into(),
            schema: "public".into(),
            port: 5432,
        }
    }
}

pub const DEFAULT_SYNC_STREAM_TIMEOUT_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub storage_root_folder: String,
    pub http_server_port: u16,
    pub http_server_host: String,
    pub log_requests: bool,
    pub log_level: String,
    pub use_compression_middleware: bool,

    pub eth_network: String,
    pub decentraland_address: String,
    pub additional_decentraland_address: Option<String>,
    pub ignore_blockchain_access_checks: Option<String>,
    pub l1_http_provider_url: String,
    pub l2_http_provider_url: String,

    pub ens_owner_provider_url: String,
    pub land_manager_subgraph_url: String,
    pub collections_l1_subgraph_url: String,
    pub collections_l2_subgraph_url: String,
    pub third_party_registry_l2_subgraph_url: String,
    pub blocks_l1_subgraph_url: String,
    pub blocks_l2_subgraph_url: String,

    pub psql_password: String,
    pub psql_user: String,
    pub psql_database: String,
    pub psql_host: String,
    pub psql_schema: String,
    pub psql_port: u16,
    pub pg_idle_timeout_ms: u64,
    pub pg_query_timeout_ms: u64,
    pub pg_stream_query_timeout_ms: u64,

    pub disable_synchronization: bool,
    pub sync_stream_timeout: String,
    pub bootstrap_from_scratch: bool,
    pub content_server_address: String,
    pub custom_dao: Option<String>,
    pub update_from_dao_interval_ms: u64,
    pub sync_ignored_entity_types: String,

    pub deployments_default_rate_limit_ttl: u64,
    pub deployments_default_rate_limit_max: u64,
    pub deployment_rate_limit_max: HashMap<EntityType, u64>,
    pub deployment_rate_limit_ttl: HashMap<EntityType, u64>,

    pub entities_cache_size: u32,
    pub fetch_request_timeout: String,
    pub request_ttl_backwards_ms: u64,
    pub garbage_collection: bool,
    pub garbage_collection_interval_ms: u64,
    pub snapshot_frequency_ms: u64,
    pub validate_api: bool,
    pub folder_migration_max_concurrency: u32,
    pub retry_failed_deployments_delay_time_ms: u64,
    pub read_only: bool,
    pub subgraph_component_retries: String,
    pub subgraph_component_query_timeout_ms: u64,

    pub denylist_file_name: String,
    pub denylist_urls: String,

    pub storage_decompress_cache_ttl_ms: Option<u64>,
    pub storage_decompress_cache_max_size: Option<u64>,
    pub storage_decompress_cache_eviction_interval_ms: Option<u64>,
}

impl EnvironmentConfig {
    pub fn from_env() -> Self {
        let eth_network = env_or("ETH_NETWORK", DEFAULT_ETH_NETWORK);
        let is_mainnet = eth_network == "mainnet";

        let http_server_port: u16 = env_or("HTTP_SERVER_PORT", &DEFAULT_HTTP_SERVER_PORT.to_string())
            .parse()
            .unwrap_or(DEFAULT_HTTP_SERVER_PORT);

        Self {
            storage_root_folder: env_or("STORAGE_ROOT_FOLDER", DEFAULT_STORAGE_ROOT_FOLDER),
            http_server_port,
            http_server_host: env_or("HTTP_SERVER_HOST", DEFAULT_HTTP_SERVER_HOST),
            log_requests: env_or("LOG_REQUESTS", "true") != "false",
            log_level: env_or("LOG_LEVEL", "INFO"),
            use_compression_middleware: env_or("USE_COMPRESSION_MIDDLEWARE", "false") == "true",

            eth_network: eth_network.clone(),
            decentraland_address: DECENTRALAND_ADDRESS.to_string(),
            additional_decentraland_address: std::env::var("ADDITIONAL_DECENTRALAND_ADDRESS").ok(),
            ignore_blockchain_access_checks: std::env::var("IGNORE_BLOCKCHAIN_ACCESS_CHECKS").ok(),
            l1_http_provider_url: env_or(
                "L1_HTTP_PROVIDER_URL",
                if is_mainnet {
                    "https://rpc.decentraland.org/mainnet?project=catalyst-content"
                } else {
                    "https://rpc.decentraland.org/sepolia?project=catalyst-content"
                },
            ),
            l2_http_provider_url: env_or(
                "L2_HTTP_PROVIDER_URL",
                if is_mainnet {
                    "https://rpc.decentraland.org/polygon?project=catalyst-content"
                } else {
                    "https://rpc.decentraland.org/amoy?project=catalyst-content"
                },
            ),

            ens_owner_provider_url: env_or(
                "ENS_OWNER_PROVIDER_URL",
                if is_mainnet { DEFAULT_ENS_OWNER_PROVIDER_URL_MAINNET } else { DEFAULT_ENS_OWNER_PROVIDER_URL_TESTNET },
            ),
            land_manager_subgraph_url: env_or(
                "LAND_MANAGER_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_LAND_MANAGER_SUBGRAPH_MAINNET } else { DEFAULT_LAND_MANAGER_SUBGRAPH_TESTNET },
            ),
            collections_l1_subgraph_url: env_or(
                "COLLECTIONS_L1_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_COLLECTIONS_SUBGRAPH_MAINNET } else { DEFAULT_COLLECTIONS_SUBGRAPH_TESTNET },
            ),
            collections_l2_subgraph_url: env_or(
                "COLLECTIONS_L2_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_COLLECTIONS_SUBGRAPH_MATIC_MAINNET } else { DEFAULT_COLLECTIONS_SUBGRAPH_MATIC_AMOY },
            ),
            third_party_registry_l2_subgraph_url: env_or(
                "THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_THIRD_PARTY_REGISTRY_SUBGRAPH_MATIC_MAINNET } else { DEFAULT_THIRD_PARTY_REGISTRY_SUBGRAPH_MATIC_AMOY },
            ),
            blocks_l1_subgraph_url: env_or(
                "BLOCKS_L1_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_BLOCKS_SUBGRAPH_MAINNET } else { DEFAULT_BLOCKS_SUBGRAPH_TESTNET },
            ),
            blocks_l2_subgraph_url: env_or(
                "BLOCKS_L2_SUBGRAPH_URL",
                if is_mainnet { DEFAULT_BLOCKS_SUBGRAPH_MATIC_MAINNET } else { DEFAULT_BLOCKS_SUBGRAPH_MATIC_AMOY },
            ),

            psql_password: env_or("POSTGRES_CONTENT_PASSWORD", &DatabaseConfig::default().password),
            psql_user: env_or("POSTGRES_CONTENT_USER", &DatabaseConfig::default().user),
            psql_database: env_or("POSTGRES_CONTENT_DB", &DatabaseConfig::default().database),
            psql_host: env_or("POSTGRES_HOST", &DatabaseConfig::default().host),
            psql_schema: env_or("POSTGRES_SCHEMA", &DatabaseConfig::default().schema),
            psql_port: env_or("POSTGRES_PORT", &DatabaseConfig::default().port.to_string())
                .parse()
                .unwrap_or(DatabaseConfig::default().port),
            pg_idle_timeout_ms: parse_duration_ms_or("PG_IDLE_TIMEOUT", 30_000),
            pg_query_timeout_ms: parse_duration_ms_or("PG_QUERY_TIMEOUT", 60_000),
            pg_stream_query_timeout_ms: parse_duration_ms_or("PG_STREAM_QUERY_TIMEOUT", 600_000),

            disable_synchronization: env_or("DISABLE_SYNCHRONIZATION", "false") == "true",
            sync_stream_timeout: env_or("SYNC_STREAM_TIMEOUT", "10m"),
            bootstrap_from_scratch: env_or("BOOTSTRAP_FROM_SCRATCH", "false") == "true",
            content_server_address: std::env::var("CONTENT_SERVER_ADDRESS").unwrap_or_else(|_| {
                format!("http://127.0.0.1:{}", http_server_port)
            }),
            custom_dao: std::env::var("CUSTOM_DAO").ok(),
            update_from_dao_interval_ms: env_or("UPDATE_FROM_DAO_INTERVAL", "1800000")
                .parse()
                .unwrap_or(1_800_000),
            sync_ignored_entity_types: env_or("SYNC_IGNORED_ENTITY_TYPES", ""),

            deployments_default_rate_limit_ttl: parse_duration_ms_or(
                "DEPLOYMENTS_DEFAULT_RATE_LIMIT_TTL",
                60_000,
            ) / 1000,
            deployments_default_rate_limit_max: env_or("DEPLOYMENTS_DEFAULT_RATE_LIMIT_MAX", "300")
                .parse()
                .unwrap_or(300),
            deployment_rate_limit_max: parse_entity_type_map("DEPLOYMENT_RATE_LIMIT_MAX_"),
            deployment_rate_limit_ttl: parse_entity_type_map("DEPLOYMENT_RATE_LIMIT_TTL_"),

            entities_cache_size: env_or("ENTITIES_CACHE_SIZE", &DEFAULT_ENTITIES_CACHE_SIZE.to_string())
                .parse()
                .unwrap_or(DEFAULT_ENTITIES_CACHE_SIZE),
            fetch_request_timeout: env_or("FETCH_REQUEST_TIMEOUT", "2m"),
            request_ttl_backwards_ms: 20 * 60 * 1000,
            garbage_collection: env_or("GARBAGE_COLLECTION", "false") == "true",
            garbage_collection_interval_ms: env_or("GARBAGE_COLLECTION_INTERVAL", "21600000")
                .parse()
                .unwrap_or(6 * 60 * 60 * 1000),
            snapshot_frequency_ms: env_or("SNAPSHOT_FREQUENCY_IN_MILLISECONDS", "21600000")
                .parse()
                .unwrap_or(6 * 60 * 60 * 1000),
            validate_api: env_or("VALIDATE_API", "false") == "true",
            folder_migration_max_concurrency: env_or(
                "FOLDER_MIGRATION_MAX_CONCURRENCY",
                &DEFAULT_FOLDER_MIGRATION_MAX_CONCURRENCY.to_string(),
            )
            .parse()
            .unwrap_or(DEFAULT_FOLDER_MIGRATION_MAX_CONCURRENCY),
            retry_failed_deployments_delay_time_ms: env_or(
                "RETRY_FAILED_DEPLOYMENTS_DELAY_TIME",
                "900000",
            )
            .parse()
            .unwrap_or(15 * 60 * 1000),
            read_only: env_or("READ_ONLY", "false") == "true",
            subgraph_component_retries: env_or("SUBGRAPH_COMPONENT_RETRIES", "1"),
            subgraph_component_query_timeout_ms: env_or(
                "SUBGRAPH_COMPONENT_QUERY_TIMEOUT",
                "60000",
            )
            .parse()
            .unwrap_or(60_000),

            denylist_file_name: env_or("DENYLIST_FILE_NAME", DEFAULT_DENYLIST_FILE_NAME),
            denylist_urls: env_or("DENYLIST_URLS", DEFAULT_DENYLIST_URLS),

            storage_decompress_cache_ttl_ms: std::env::var("STORAGE_DECOMPRESS_CACHE_TTL")
                .ok()
                .and_then(|v| v.parse().ok()),
            storage_decompress_cache_max_size: std::env::var("STORAGE_DECOMPRESS_CACHE_MAX_SIZE")
                .ok()
                .and_then(|v| v.parse().ok()),
            storage_decompress_cache_eviction_interval_ms: std::env::var(
                "STORAGE_DECOMPRESS_CACHE_EVICTION_INTERVAL",
            )
            .ok()
            .and_then(|v| v.parse().ok()),
        }
    }

    pub fn is_mainnet(&self) -> bool {
        self.eth_network == "mainnet"
    }
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parse_duration_ms_or(key: &str, default_ms: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default_ms)
}

fn parse_entity_type_map(prefix: &str) -> HashMap<EntityType, u64> {
    let mut map = HashMap::new();
    for (key, value) in std::env::vars() {
        if let Some(suffix) = key.strip_prefix(prefix) {
            if let (Some(entity_type), Ok(num)) = (EntityType::parse(suffix), value.parse::<u64>())
            {
                map.insert(entity_type, num);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_config_default() {
        let cfg = DatabaseConfig::default();
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.database, "content");
    }

    #[test]
    fn env_or_returns_default_when_unset() {
        let val = env_or("CATALYRST_TYPES_TEST_NONEXISTENT_12345", "fallback");
        assert_eq!(val, "fallback");
    }
}
