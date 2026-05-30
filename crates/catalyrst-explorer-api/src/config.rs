use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_host: String,
    pub http_port: u16,
    pub realm_name: String,
    pub catalyst_url: String,
    pub lambdas_url: String,
    pub comms_url: String,
    pub upstream_marketplace_url: String,
    pub upstream_builder_url: String,
    pub upstream_worlds_url: String,
    pub network_id: u64,
    pub env_name: String,
    pub public_realm_url: String,
    pub bff_url: String,
    pub comms_adapter: String,
    pub comms_fixed_adapter: String,
    pub feature_flags_config_path: String,
    pub blocklist_path: String,
    /// HTTP base for the live archipelago /hot-scenes endpoint (explore bundle).
    /// The realm-provider proxies it so the explorer sees populated scenes.
    pub hot_scenes_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            http_host: env::var("HTTP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            http_port: env::var("HTTP_SERVER_PORT").unwrap_or_else(|_| "5137".into())
                .parse().context("HTTP_SERVER_PORT must be u16")?,
            realm_name: env::var("REALM_NAME").unwrap_or_else(|_| "catalyrst".into()),
            catalyst_url: env::var("CATALYST_URL").unwrap_or_else(|_| "http://127.0.0.1:5140".into()),
            lambdas_url: env::var("LAMBDAS_URL").unwrap_or_else(|_| "http://127.0.0.1:5142".into()),
            comms_url: env::var("COMMS_URL").unwrap_or_else(|_| "http://127.0.0.1:5137/comms".into()),
            upstream_marketplace_url: env::var("UPSTREAM_MARKETPLACE_URL")
                .unwrap_or_else(|_| "https://marketplace-api.decentraland.org".into()),
            upstream_builder_url: env::var("UPSTREAM_BUILDER_URL")
                .unwrap_or_else(|_| "https://builder-api.decentraland.org".into()),
            upstream_worlds_url: env::var("UPSTREAM_WORLDS_URL")
                .unwrap_or_else(|_| "https://worlds-content-server.decentraland.org".into()),
            network_id: env::var("NETWORK_ID").unwrap_or_else(|_| "1".into())
                .parse().context("NETWORK_ID must be u64")?,
            env_name: env::var("ENV_NAME").unwrap_or_else(|_| "prd".into()),
            public_realm_url: env::var("PUBLIC_REALM_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5137".into()),
            bff_url: env::var("BFF_URL").unwrap_or_else(|_| "/bff".into()),
            comms_adapter: env::var("COMMS_ADAPTER")
                .unwrap_or_else(|_| "offline:offline".into()),
            comms_fixed_adapter: env::var("COMMS_FIXED_ADAPTER")
                .unwrap_or_else(|_| "offline:offline".into()),
            feature_flags_config_path: env::var("FEATURE_FLAGS_CONFIG_PATH")
                .unwrap_or_else(|_| "./config/feature-flags.json".into()),
            blocklist_path: env::var("BLOCKLIST_PATH")
                .unwrap_or_else(|_| "./config/denylist.json".into()),
            hot_scenes_url: env::var("HOT_SCENES_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5143/hot-scenes".into()),
        })
    }
}
