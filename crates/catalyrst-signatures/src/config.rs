use anyhow::Result;
use catalyrst_envcfg::{get_int, get_port, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    pub chain_name: String,

    pub marketplace_subgraph_url: Option<String>,

    pub rentals_subgraph_url: Option<String>,

    pub squid_database_url: Option<String>,

    pub squid_schema: String,

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
            auth_expiration_secs: get_int("AUTH_EXPIRATION_SECONDS", 5 * 60)?,
        })
    }
}
