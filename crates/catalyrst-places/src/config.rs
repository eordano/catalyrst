use anyhow::Result;
use catalyrst_envcfg::{env_bool, get_port, local_endpoint, required};
use std::env;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub places_database_url: String,
    pub places_writer_database_url: Option<String>,
    pub content_database_url: Option<String>,
    pub derive_places_from_content: bool,
    pub content_public_url: String,
    pub mirror_upstream: bool,
    pub upstream_url: String,
    pub worlds_mirror_upstream: bool,
    pub worlds_upstream_url: String,
    pub worlds_mirror_interval_secs: u64,
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
            content_database_url: env::var("CONTENT_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            derive_places_from_content: env_bool("PLACES_DERIVE_FROM_CONTENT", false),
            content_public_url: env::var("CONTENT_PUBLIC_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "/content".to_string()),
            mirror_upstream: env_bool("PLACES_MIRROR_UPSTREAM", false),
            upstream_url: env::var("PLACES_UPSTREAM_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://places.decentraland.org".to_string()),
            worlds_mirror_upstream: env_bool("WORLDS_MIRROR_UPSTREAM", false),
            worlds_upstream_url: env::var("WORLDS_UPSTREAM_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://places.decentraland.org".to_string()),
            worlds_mirror_interval_secs: env::var("WORLDS_MIRROR_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .filter(|&s| s > 0)
                .unwrap_or(3600),
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
            comms_gatekeeper_url: local_endpoint("COMMS_GATEKEEPER_URL", 5138),
            events_api_url: local_endpoint("EVENTS_API_URL", 5135),
            presence_url: env::var("PRESENCE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "http://127.0.0.1:5152".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror loop sleeps this interval between passes, so a zero (or
    /// unparsable) WORLDS_MIRROR_INTERVAL_SECS must fall back to the default
    /// rather than busy-loop.
    #[test]
    fn worlds_mirror_interval_rejects_zero_and_garbage() {
        env::set_var(
            "PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING",
            "postgres://config-test",
        );
        for bad in ["0", " 0 ", "garbage", "-5", ""] {
            env::set_var("WORLDS_MIRROR_INTERVAL_SECS", bad);
            assert_eq!(
                Config::from_env().unwrap().worlds_mirror_interval_secs,
                3600,
                "{bad:?} must fall back to the default"
            );
        }
        env::set_var("WORLDS_MIRROR_INTERVAL_SECS", "60");
        assert_eq!(Config::from_env().unwrap().worlds_mirror_interval_secs, 60);
        env::remove_var("WORLDS_MIRROR_INTERVAL_SECS");
        assert_eq!(
            Config::from_env().unwrap().worlds_mirror_interval_secs,
            3600
        );
    }
}
