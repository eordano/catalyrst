#![allow(clippy::result_large_err)]

pub mod auth;
pub mod ban;
pub mod config;
pub mod content;
pub mod control_extensions;
pub mod control_v4;
pub mod feed;
pub mod handlers;
pub mod livekit;
pub mod nats;
pub mod peers;
pub mod proto;
pub mod registry;
pub mod session;
pub mod state;
pub mod ws;

pub use config::Config;
pub use state::{AppState, AppStateInner};

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::serve::{ListenerExt, TapIo};
use axum::Router;
use catalyrst_envcfg::service_scaffold::cors_any_methods;
use tokio::net::{TcpListener, TcpStream};
use tower_http::cors::Any;

use crate::auth::ChallengeStore;
use crate::ban::{BanChecker, DenyList};
use crate::content::ContentResolver;
use crate::control_v4::AssignmentAuthority;
use crate::feed::FeedCache;
use crate::livekit::LivekitMinter;
use crate::nats::{FeedPublisher, NatsBus};
use crate::peers::PeerDirectory;
use crate::registry::PeersRegistry;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let bus = NatsBus::new(cfg.nats.clone());
    let state = build_state_with(cfg, Arc::clone(&bus) as Arc<dyn FeedPublisher>).await?;
    bus.start(Arc::clone(&state));
    Ok(state)
}

/// Wires everything but the broker link, so a test can drive the socket layer through an
/// in-process publisher and feed messages in by hand.
pub async fn build_state_with(cfg: &Config, publisher: Arc<dyn FeedPublisher>) -> Result<AppState> {
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
    let registry = PeersRegistry::new();
    let peers = PeerDirectory::new(
        cfg.cluster.clone(),
        Arc::clone(&livekit),
        Arc::clone(&ban_checker),
        Arc::clone(&registry),
    );
    let _expiry_task = Arc::clone(&peers).spawn_expiry();
    let _ban_sweep_task = Arc::clone(&peers).spawn_ban_sweep();

    let challenges = ChallengeStore::new(cfg.auth.clone());

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
    let replica_id = uuid::Uuid::new_v4().to_string();
    let control_v4 = match &cfg.control_database_url {
        Some(url) if !cfg.server.control_v4_audience.trim().is_empty() => {
            let settings = catalyrst_db::PoolSettings {
                max_connections: 5,
                ..catalyrst_db::PoolSettings::default()
            };
            match catalyrst_db::connect_pool(url, &settings).await {
                Ok(pool) => match AssignmentAuthority::pg_with_namespace(
                    pool,
                    cfg.server.control_v4_audience.clone(),
                )
                .await
                {
                    Ok(authority) => authority,
                    Err(_) => {
                        tracing::error!(
                            "v4 control authority schema could not be prepared; /ws/v4 will fail closed for ownership"
                        );
                        AssignmentAuthority::unavailable(&replica_id)
                    }
                },
                Err(e @ catalyrst_db::PoolError::InvalidUrl(_)) => {
                    return Err(e).context("invalid v4 control DB connection string");
                }
                Err(catalyrst_db::PoolError::Connect(_)) => {
                    tracing::error!(
                        "v4 control DB unavailable; /ws/v4 will fail closed for ownership"
                    );
                    AssignmentAuthority::unavailable(&replica_id)
                }
            }
        }
        Some(_) => {
            tracing::error!(
                "ARCHIPELAGO_CONTROL_PG_CONNECTION_STRING is set but v4 audience is empty; /ws/v4 will fail closed for ownership"
            );
            AssignmentAuthority::unavailable(&replica_id)
        }
        None => {
            tracing::warn!(
                "ARCHIPELAGO_CONTROL_PG_CONNECTION_STRING is unset \u{2014} /ws/v4 remains reachable but control ownership and assignment delivery fail closed"
            );
            AssignmentAuthority::unavailable(&replica_id)
        }
    };

    tracing::info!(
        livekit_armed = livekit.is_armed(),
        ban_check_armed = ban_checker.is_armed(),
        deny_list_armed = deny_list.is_armed(),
        feed_armed = publisher.is_enabled(),
        auth_required = challenges.required(),
        content_armed = content.is_armed(),
        control_v4_armed = control_v4.is_available(),
        "catalyrst-archipelago wired"
    );

    Ok(Arc::new(AppStateInner {
        cfg: cfg.clone(),
        peers,
        registry,
        feed: Arc::new(FeedCache::default()),
        publisher,
        challenges,
        livekit,
        content,
        control_v4,
        replica_id,
        ban_checker,
        deny_list,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(handlers::routes())
        .merge(ws::routes())
        .layer(cors_any_methods().expose_headers(Any))
}

pub fn low_latency_listener(listener: TcpListener) -> TapIo<TcpListener, fn(&mut TcpStream)> {
    listener.tap_io(|stream| {
        if let Err(error) = stream.set_nodelay(true) {
            tracing::warn!(%error, "failed to enable TCP_NODELAY on control connection");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::serve::Listener;

    #[tokio::test]
    async fn accepted_control_connections_send_without_nagle_delay() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut listener =
                low_latency_listener(TcpListener::bind("127.0.0.1:0").await.unwrap());
            let address = listener.local_addr().unwrap();
            for _ in 0..3 {
                let client = TcpStream::connect(address).await.unwrap();
                let (stream, peer) = listener.accept().await;
                assert!(stream.nodelay().unwrap());
                assert_eq!(peer, client.local_addr().unwrap());
                assert_eq!(stream.local_addr().unwrap(), address);
            }
        })
        .await
        .unwrap();
    }
}
