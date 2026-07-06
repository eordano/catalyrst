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

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

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
            let opts = PgConnectOptions::from_str(url)
                .context("invalid content DB connection string")?
                .options([
                    ("statement_timeout", "60000"),
                    ("idle_in_transaction_session_timeout", "30000"),
                ]);
            match PgPoolOptions::new()
                .max_connections(5)
                .idle_timeout(Duration::from_secs(30))
                .connect_with(opts)
                .await
            {
                Ok(pool) => Some(pool),
                Err(e) => {
                    tracing::warn!(error = %e, "content DB unavailable — /hot-scenes scene resolution disabled");
                    None
                }
            }
        }
        None => {
            tracing::warn!("content DB unconfigured — /hot-scenes scene resolution disabled");
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
        .merge(handlers::status_routes())
        .merge(handlers::api_routes())
        .merge(ws::routes())
}
