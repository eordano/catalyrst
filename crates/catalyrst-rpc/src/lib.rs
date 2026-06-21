pub mod config;
pub mod modules;
pub mod relay;
pub mod state;

pub use config::Config;
pub use state::{AppState, AppStateInner, READ_ONLY_METHODS};

use anyhow::Result;
use axum::Router;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

pub fn api_router() -> Router<AppState> {
    modules::rpc::routes().merge(modules::admin::routes())
}

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("catalyrst-rpc/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(16)
        // Keep upstream connections warm so each relayed call reuses a pooled
        // TLS connection instead of re-paying DNS + TLS handshake (the source of
        // the /ethereum cold-call variance: ~140ms warm vs ~385ms cold).
        .pool_idle_timeout(std::time::Duration::from_secs(300))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .build()?;

    for (network, url) in &cfg.upstreams {
        if url.trim().is_empty() {
            tracing::warn!(%network, "upstream URL is empty; requests for this network will fail");
        }
        if config::chain_id_for(network).is_none() {
            tracing::warn!(%network, "no known chain id for configured network");
        }
    }

    let admin_token = std::env::var("CATALYRST_RPC_ADMIN_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    if admin_token.is_none() {
        tracing::warn!(
            "CATALYRST_RPC_ADMIN_TOKEN unset; /admin/rpc/* routes will fail closed (403)"
        );
    }

    let allowed_methods: BTreeSet<String> =
        READ_ONLY_METHODS.iter().map(|m| m.to_string()).collect();
    let upstreams: BTreeMap<String, String> = cfg
        .upstreams
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    tracing::info!(
        networks = ?upstreams.keys().collect::<Vec<_>>(),
        methods = allowed_methods.len(),
        "catalyrst-rpc wired"
    );

    Ok(Arc::new(AppStateInner {
        cfg,
        http,
        allowed_methods: RwLock::new(allowed_methods),
        upstreams: RwLock::new(upstreams),
        admin_token,
    }))
}
