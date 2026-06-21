use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    /// The chain the listing signatures are scoped to. Determines the Rentals
    /// EIP-712 verifying contract + chainId used to recover the lessor address.
    /// Upstream calls this CHAIN_NAME ("ETHEREUM_MAINNET" | "ETHEREUM_SEPOLIA").
    pub chain_name: String,

    /// Optional marketplace subgraph URL. Legacy/unused locally — the
    /// NFT-ownership + estate-size + metadata cross-checks now run against the
    /// local squid marketplace DB (`squid_database_url`) instead of an external
    /// subgraph. Kept for env parity with upstream MARKETPLACE_SUBGRAPH_URL.
    pub marketplace_subgraph_url: Option<String>,
    /// Optional rentals subgraph URL. The "rental already on-chain" check needs
    /// the rentals indexer, which is NOT mirrored into the local squid DB (only
    /// the marketplace `nft` table is). When unset (the local default), the
    /// on-chain rental check falls back to the DB unique-open-rental constraint;
    /// the PATCH refresh path still re-resolves NFT metadata from squid. Kept
    /// for env parity with upstream RENTALS_SUBGRAPH_URL.
    pub rentals_subgraph_url: Option<String>,

    /// Local squid marketplace DB connection (the `nft` table). When set, the
    /// create-time NFT-ownership / existence / estate-size checks and the PATCH
    /// metadata refresh run against it via sqlx. Upstream resolved these from
    /// the marketplace subgraph; we read the same data from the local index.
    pub squid_database_url: Option<String>,
    /// Schema holding the squid marketplace tables. Default `squid_marketplace`.
    pub squid_schema: String,

    /// Auth-chain signature expiration window (seconds). Upstream uses 5 min.
    pub auth_expiration_secs: i64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5151)?,
            database_url: required("SIGNATURES_PG_CONNECTION_STRING")?,
            chain_name: env::var("CHAIN_NAME").unwrap_or_else(|_| "ETHEREUM_MAINNET".to_string()),
            marketplace_subgraph_url: env::var("MARKETPLACE_SUBGRAPH_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            rentals_subgraph_url: env::var("RENTALS_SUBGRAPH_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            squid_database_url: env::var("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            squid_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),
            auth_expiration_secs: get_i64("AUTH_EXPIRATION_SECONDS", 5 * 60)?,
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

fn get_i64(key: &str, default: i64) -> Result<i64> {
    match env::var(key) {
        Ok(s) => s.parse::<i64>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}
