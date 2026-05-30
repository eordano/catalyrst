use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub schema: String,
    pub refresh_interval_secs: u64,
    pub land_contract_address: String,
    pub estate_contract_address: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5152)?,
            database_url: required("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),
            // The dataset TTL: how often the full-atlas snapshot (and the
            // JSON/PNG byte caches keyed off it) is rebuilt from squid.
            // `MAP_TILES_TTL_SECONDS` is the documented knob; the older
            // `MAP_REFRESH_INTERVAL_SECS` is kept as an alias for parity with
            // existing deployments. Default 60s.
            refresh_interval_secs: env::var("MAP_TILES_TTL_SECONDS")
                .or_else(|_| env::var("MAP_REFRESH_INTERVAL_SECS"))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            land_contract_address: env::var("LAND_CONTRACT_ADDRESS")
                .unwrap_or_else(|_| "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".to_string()),
            estate_contract_address: env::var("ESTATE_CONTRACT_ADDRESS")
                .unwrap_or_else(|_| "0x959e104e1a4db6317fa58f8295f586e1a978c297".to_string()),
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
