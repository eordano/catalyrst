use anyhow::Result;
use catalyrst_envcfg::{get_port, get_u64, local_endpoint, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub database_url: String,

    pub archipelago_url: String,

    pub comms_url: String,

    pub worlds_server_url: String,

    pub genesis_realm: String,

    pub snapshot_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5152)?,
            database_url: required("PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            archipelago_url: trim_url(local_endpoint("ARCHIPELAGO_URL", 5139)),
            comms_url: trim_url(local_endpoint("COMMS_URL", 5138)),
            worlds_server_url: trim_url(local_endpoint("WORLDS_SERVER_URL", 5142)),
            genesis_realm: env::var("PRESENCE_GENESIS_REALM")
                .unwrap_or_else(|_| "main".to_string()),
            snapshot_interval_secs: get_u64("PRESENCE_SNAPSHOT_INTERVAL_SECS", 300)?,
        })
    }
}

fn trim_url(s: String) -> String {
    s.trim_end_matches('/').to_string()
}
