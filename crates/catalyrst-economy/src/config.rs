use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub dapps_database_url: String,
    pub dapps_schema: String,
    pub squid_schema: String,

    pub api_version: String,
    pub min_sale_value_in_wei: String,
    pub max_transactions_per_day: i64,
    pub contract_addresses_url: String,
    pub contract_addresses_chain_key: String,
    pub collections_chain_id: u64,
    pub collections_fetch_interval_ms: u64,

    pub rpc_url: Option<String>,
    pub max_gas_price_allowed_in_wei: Option<u128>,
    /// Hard cap on the gas LIMIT of a self-broadcast meta-tx. A funded relayer
    /// pays gas, so an expensive (but allowlisted) call could otherwise drain
    /// the key; the validation phase only caps gas PRICE, not the limit. Default
    /// 1_500_000 covers DCL single-item marketplace/store/bid meta-tx with
    /// headroom; raise via MAX_GAS_LIMIT for batch operations.
    pub max_gas_limit: u64,

    pub relayer_url: Option<String>,
    pub relayer_id: Option<String>,
    pub relayer_api_key: Option<String>,
    pub relayer_speed: String,
    pub relayer_max_status_checks: u32,
    pub relayer_sleep_ms: u64,

    // Direct (self-hosted) JSON-RPC broadcast path. Off unless explicitly
    // enabled AND a relayer key is present; see ports::signer::DirectSigner.
    pub meta_tx_broadcast_enabled: bool,
    pub relayer_private_key: Option<String>,

    /// Bearer token gating the runtime relayer admin routes
    /// (`/{api}/admin/relayer*`). Unset ⇒ those routes 403 (fail-closed); the
    /// console hides the controls. The crate had no prior admin token, so this
    /// is its own dedicated env (per the per-crate `CATALYRST_<X>_ADMIN_TOKEN`
    /// convention used by catalyrst-badges / catalyrst-builder).
    pub admin_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5155)?,

            dapps_database_url: required("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            dapps_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "marketplace".to_string()),
            squid_schema: env::var("SQUID_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),

            api_version: env::var("API_VERSION").unwrap_or_else(|_| "v1".to_string()),
            min_sale_value_in_wei: env::var("MIN_SALE_VALUE_IN_WEI")
                .unwrap_or_else(|_| "1000000000000000000".to_string()),
            max_transactions_per_day: get_i64("MAX_TRANSACTIONS_PER_DAY", 10)?,
            contract_addresses_url: env::var("CONTRACT_ADDRESSES_URL")
                .unwrap_or_else(|_| "https://contracts.decentraland.org/addresses.json".to_string()),
            contract_addresses_chain_key: env::var("CONTRACT_ADDRESSES_CHAIN_KEY")
                .unwrap_or_else(|_| "matic".to_string()),
            collections_chain_id: get_u64("COLLECTIONS_CHAIN_ID", 137)?,
            collections_fetch_interval_ms: get_u64("COLLECTIONS_FETCH_INTERVAL_MS", 3_600_000)?,

            rpc_url: opt("RPC_URL"),
            max_gas_price_allowed_in_wei: match env::var("MAX_GAS_PRICE_ALLOWED_IN_WEI") {
                Ok(s) if !s.is_empty() => Some(
                    s.parse::<u128>()
                        .context("invalid MAX_GAS_PRICE_ALLOWED_IN_WEI")?,
                ),
                _ => None,
            },
            max_gas_limit: get_u64("MAX_GAS_LIMIT", 1_500_000)?,

            relayer_url: opt("OZ_RELAYER_URL"),
            relayer_id: opt("OZ_RELAYER_ID"),
            relayer_api_key: opt("OZ_RELAYER_API_KEY"),
            relayer_speed: env::var("OZ_RELAYER_SPEED").unwrap_or_else(|_| "fast".to_string()),
            relayer_max_status_checks: get_u32("OZ_MAX_STATUS_CHECKS", 150)?,
            relayer_sleep_ms: get_u64("OZ_SLEEP_TIME_BETWEEN_CHECKS_MS", 800)?,

            meta_tx_broadcast_enabled: get_bool("META_TX_BROADCAST_ENABLED", false)?,
            relayer_private_key: opt("RELAYER_PRIVATE_KEY"),

            admin_token: opt("CATALYRST_ECONOMY_ADMIN_TOKEN"),
        })
    }

    pub fn has_relayer(&self) -> bool {
        let has = |o: &Option<String>| o.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        has(&self.relayer_url) && has(&self.relayer_id) && has(&self.relayer_api_key)
    }

    pub fn has_rpc(&self) -> bool {
        self.rpc_url.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("missing required env var: {}", key))
}

fn opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
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

fn get_u32(key: &str, default: u32) -> Result<u32> {
    match env::var(key) {
        Ok(s) => s.parse::<u32>().with_context(|| format!("invalid {}", key)),
        Err(_) => Ok(default),
    }
}

fn get_bool(key: &str, default: bool) -> Result<bool> {
    match env::var(key) {
        Ok(s) => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" | "" => Ok(false),
            other => Err(anyhow!("invalid {}: expected a boolean, got {:?}", key, other)),
        },
        Err(_) => Ok(default),
    }
}
