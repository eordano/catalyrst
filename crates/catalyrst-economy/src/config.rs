use alloy::primitives::U256;
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

    pub max_gas_limit: u64,

    pub relayer_url: Option<String>,
    pub relayer_id: Option<String>,
    pub relayer_api_key: Option<String>,
    pub relayer_speed: String,
    pub relayer_max_status_checks: u32,
    pub relayer_sleep_ms: u64,

    pub meta_tx_broadcast_enabled: bool,
    pub relayer_private_key: Option<String>,

    pub admin_token: Option<String>,

    pub landiler_escrow_address: Option<String>,

    pub names_chain_id: u64,
    pub eth_rpc_url: Option<String>,
    pub names_max_price_wei: Option<U256>,

    pub receipt_poll_interval_ms: u64,
    pub receipt_timeout_ms: u64,
    pub broker_reconcile_interval_ms: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let cfg = Self {
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
            contract_addresses_url: env::var("CONTRACT_ADDRESSES_URL").unwrap_or_else(|_| {
                "https://contracts.decentraland.org/addresses.json".to_string()
            }),
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

            landiler_escrow_address: opt("LANDILER_ESCROW_ADDRESS"),

            names_chain_id: get_u64("NAMES_CHAIN_ID", 1)?,
            eth_rpc_url: opt("ETH_RPC_URL"),
            names_max_price_wei: match opt("NAMES_MAX_PRICE_WEI") {
                Some(raw) => Some(
                    U256::from_str_radix(raw.trim(), 10)
                        .context("invalid NAMES_MAX_PRICE_WEI: not a decimal integer")?,
                ),
                None => None,
            },

            receipt_poll_interval_ms: get_u64("RECEIPT_POLL_INTERVAL_MS", 3_000)?,
            receipt_timeout_ms: get_u64("RECEIPT_TIMEOUT_MS", 180_000)?,
            broker_reconcile_interval_ms: get_u64("BROKER_RECONCILE_INTERVAL_MS", 60_000)?,
        };
        guard_admin_exposure(
            &cfg.http_host,
            cfg.admin_token.as_deref(),
            "CATALYRST_ECONOMY_ADMIN_TOKEN",
        )?;
        Ok(cfg)
    }

    pub fn has_relayer(&self) -> bool {
        let has = |o: &Option<String>| o.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        has(&self.relayer_url) && has(&self.relayer_id) && has(&self.relayer_api_key)
    }

    pub fn has_rpc(&self) -> bool {
        self.rpc_url
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
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
            other => Err(anyhow!(
                "invalid {}: expected a boolean, got {:?}",
                key,
                other
            )),
        },
        Err(_) => Ok(default),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let h = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(h);
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

fn guard_admin_exposure(host: &str, admin_token: Option<&str>, token_env: &str) -> Result<()> {
    if is_loopback_host(host) {
        return Ok(());
    }
    if admin_token.is_none() {
        return Err(anyhow!(
            "refusing to start: HTTP_SERVER_HOST={host:?} is not a loopback address, which exposes \
             the loopback-only broker/escrow + admin money endpoints to the network, and no \
             {token_env} is set to guard them. Bind 127.0.0.1 (front the public API with nginx) or \
             set {token_env}."
        ));
    }
    tracing::warn!(
        host = %host,
        "HTTP_SERVER_HOST is non-loopback: the broker/escrow + admin money surface is reachable \
         from the network and protected only by the bearer token. Prefer binding 127.0.0.1 behind \
         nginx."
    );
    Ok(())
}

#[cfg(test)]
mod exposure_tests {
    use super::{guard_admin_exposure, is_loopback_host};

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));
        assert!(is_loopback_host("localhost"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("::"));
        assert!(!is_loopback_host("10.0.0.5"));
        assert!(!is_loopback_host(""));
    }

    #[test]
    fn guard_policy() {
        assert!(guard_admin_exposure("127.0.0.1", None, "T").is_ok());
        assert!(guard_admin_exposure("0.0.0.0", None, "T").is_err());
        assert!(guard_admin_exposure("0.0.0.0", Some("tok"), "T").is_ok());
    }
}
