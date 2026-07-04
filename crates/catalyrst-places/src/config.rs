use anyhow::{anyhow, Context, Result};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub places_database_url: String,
    pub places_writer_database_url: Option<String>,
    pub squid_database_url: Option<String>,
    pub squid_schema: String,
    pub admin_addresses: Vec<String>,
    pub data_team_auth_token: Option<String>,
    pub admin_auth_token: Option<String>,

    pub comms_gatekeeper_url: String,

    pub events_api_url: String,

    pub presence_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 5134)?,
            places_database_url: required("PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING")?,
            places_writer_database_url: env::var(
                "PLACES_PG_COMPONENT_WRITER_PSQL_CONNECTION_STRING",
            )
            .ok()
            .filter(|s| !s.trim().is_empty()),
            squid_database_url: env::var("DAPPS_PG_COMPONENT_PSQL_CONNECTION_STRING").ok(),
            squid_schema: env::var("DAPPS_PG_COMPONENT_PSQL_SCHEMA")
                .unwrap_or_else(|_| "squid_marketplace".to_string()),
            admin_addresses: env::var("PLACES_ADMIN_ADDRESSES")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|a| a.trim().to_lowercase())
                        .filter(|a| !a.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            data_team_auth_token: env::var("DATA_TEAM_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            admin_auth_token: env::var("PLACES_ADMIN_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            comms_gatekeeper_url: env::var("COMMS_GATEKEEPER_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://comms-gatekeeper.decentraland.zone".to_string()),
            events_api_url: env::var("EVENTS_API_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://events.decentraland.zone/api".to_string()),
            presence_url: env::var("PRESENCE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:5152".to_string()),
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
