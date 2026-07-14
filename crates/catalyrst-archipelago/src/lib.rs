#![allow(clippy::result_large_err)]

pub mod auth;
pub mod ban;
pub mod cluster;
pub mod config;
pub mod content;
pub mod gossip;
pub mod handlers;
pub mod livekit;
pub mod proto;
pub mod state;
pub mod ws;

pub use config::Config;
pub use state::{AppState, AppStateInner};

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::auth::ChallengeStore;
use crate::ban::{BanChecker, DenyList};
use crate::cluster::Cluster;
use crate::content::ContentResolver;
use crate::gossip::GossipBus;
use crate::livekit::LivekitMinter;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("catalyrst-archipelago/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let livekit = Arc::new(LivekitMinter::new(cfg.livekit.clone()));
    let ban_checker = BanChecker::new(cfg.livekit.comms_gatekeeper_url.clone(), http.clone());
    let deny_list = DenyList::new(cfg.auth.deny_list_url.clone(), http.clone());
    if !deny_list.is_armed() {
        tracing::warn!(
            "DENY_LIST_URL is unset \u{2014} the wallet denylist is DISARMED and no address \
             will be blocked. This used to fall back to Decentraland's production \
             denylist; set DENY_LIST_URL to a list this deployment controls to re-arm it."
        );
    }
    let cluster = Cluster::new(
        cfg.cluster.clone(),
        Arc::clone(&livekit),
        Arc::clone(&ban_checker),
    );
    let _recluster_task = Arc::clone(&cluster).spawn_periodic();

    let challenges = ChallengeStore::new(cfg.auth.clone());

    let gossip = GossipBus::new(cfg.gossip.clone(), http);
    let _gossip_task = Arc::clone(&gossip).spawn_periodic(Arc::clone(&cluster));

    let content_pool = match &cfg.content_database_url {
        Some(url) => {
            let settings = catalyrst_db::PoolSettings {
                max_connections: 5,
                ..catalyrst_db::PoolSettings::default()
            };
            match catalyrst_db::connect_pool(url, &settings).await {
                Ok(pool) => Some(pool),
                Err(e @ catalyrst_db::PoolError::InvalidUrl(_)) => {
                    return Err(e).context("invalid content DB connection string");
                }
                Err(catalyrst_db::PoolError::Connect(e)) => {
                    tracing::warn!(error = %e, "content DB unavailable \u{2014} /hot-scenes scene resolution disabled");
                    None
                }
            }
        }
        None => {
            tracing::warn!(
                "content DB unconfigured \u{2014} /hot-scenes scene resolution disabled"
            );
            None
        }
    };
    let content = ContentResolver::new(content_pool, cfg.content_base_url.clone(), 10);

    tracing::info!(
        livekit_armed = livekit.is_armed(),
        ban_check_armed = ban_checker.is_armed(),
        deny_list_armed = deny_list.is_armed(),
        gossip_armed = gossip.is_armed(),
        auth_required = challenges.required(),
        content_armed = content.is_armed(),
        "catalyrst-archipelago wired"
    );

    Ok(Arc::new(AppStateInner {
        cfg: cfg.clone(),
        cluster,
        challenges,
        livekit,
        gossip,
        content,
        ban_checker,
        deny_list,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(handlers::routes())
        .merge(ws::routes())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers(Any),
        )
}
