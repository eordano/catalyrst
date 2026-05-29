//! Runtime configuration — pulled from env vars at startup.
//!
//! Mirrors the upstream `components.ts` initial config block: HTTP server
//! host/port, three Postgres connection strings (favorites / dapps / dapps_read),
//! plus the schema names. CORS, Wert, Transak, Segment, Snapshot are omitted —
//! none are needed on the read-only path.

use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,

    pub dapps_database_url: String,
    pub dapps_schema: String,

    pub dapps_read_database_url: String,
    pub dapps_read_schema: String,

    pub favorites_database_url: String,
    pub favorites_schema: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5133)?,

            dapps_database_url: required("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            dapps_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "marketplace".to_string()),

            dapps_read_database_url: required("DAPPS_READ_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            dapps_read_schema: env::var("DAPPS_READ_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "marketplace".to_string()),

            favorites_database_url: required("FAVORITES_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            favorites_schema: env::var("FAVORITES_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "favorites".to_string()),
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
