use anyhow::Result;
use catalyrst_envcfg::{get_port, local_endpoint, required, required_endpoint};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub database_url: String,
    pub admin_token: Option<String>,
    pub communities_content_dir: PathBuf,
    pub content_database_url: Option<String>,
    pub mutes_database_url: Option<String>,
    pub content_server_address: String,
    pub cdn_url: String,
    pub global_moderators: Vec<String>,
    pub comms_gatekeeper_url: String,
    pub archipelago_stats_url: String,
    pub restricted_names: Vec<String>,

    pub places_api_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let content_dir = env::var("COMMUNITIES_CONTENT_DIR")
            .unwrap_or_else(|_| "./data/communities/content".to_string());
        let global_moderators = env::var("COMMUNITIES_GLOBAL_MODERATORS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        // Folded once here so the denylist is compared on the same reduced form submitted names are
        // (port of #483). Folding subsumes trimming, so blank entries drop out.
        let restricted_names = env::var("RESTRICTED_NAMES")
            .unwrap_or_default()
            .split(',')
            .map(crate::rest::validate::fold_name_for_comparison)
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: get_port("HTTP_SERVER_PORT", 8080)?,
            database_url: required("COMMUNITIES_PG_CONNECTION_STRING")?,
            admin_token: env::var("API_ADMIN_TOKEN").ok().filter(|s| !s.is_empty()),
            communities_content_dir: PathBuf::from(content_dir),
            mutes_database_url: env::var("MUTES_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_database_url: env::var("CONTENT_PG_CONNECTION_STRING")
                .ok()
                .filter(|s| !s.is_empty()),
            content_server_address: local_endpoint("CONTENT_SERVER_ADDRESS", 5141),
            cdn_url: required_endpoint("CDN_URL")?,
            global_moderators,
            comms_gatekeeper_url: local_endpoint("COMMS_GATEKEEPER_URL", 5138),
            archipelago_stats_url: local_endpoint("ARCHIPELAGO_STATS_URL", 5139),
            restricted_names,
            places_api_url: env::var("PLACES_API_URL").ok().filter(|s| !s.is_empty()),
        })
    }
}
