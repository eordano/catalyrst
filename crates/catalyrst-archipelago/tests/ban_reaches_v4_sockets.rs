//! A ban ends a wallet's v4 sockets as it ends its legacy ones. A v4 socket is registered beside
//! the legacy sessions and enters the peer directory only with its first position, so the sweep
//! has to find it in the registry and the kick has to reach it there.

use std::sync::Arc;

use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use catalyrst_archipelago::ban::BanChecker;
use catalyrst_archipelago::config::{ClusterConfig, LivekitConfig};
use catalyrst_archipelago::livekit::LivekitMinter;
use catalyrst_archipelago::peers::PeerDirectory;
use catalyrst_archipelago::registry::{PeersRegistry, SocketEvent};

async fn bans(Path(address): Path<String>) -> Json<Value> {
    let is_banned = address.to_ascii_lowercase().contains("banned");
    Json(json!({ "data": { "isBanned": is_banned } }))
}

async fn gatekeeper() -> String {
    let app = Router::new().route("/users/{address}/bans", get(bans));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the gatekeeper stand-in");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}")
}

fn directory(gatekeeper: String, registry: &Arc<PeersRegistry>) -> Arc<PeerDirectory> {
    PeerDirectory::new(
        ClusterConfig::default(),
        Arc::new(LivekitMinter::new(LivekitConfig {
            api_key: None,
            api_secret: None,
            ws_url: "wss://lk.example".into(),
            token_ttl_secs: 60,
            comms_gatekeeper_url: None,
        })),
        BanChecker::new(Some(gatekeeper), reqwest::Client::new()),
        Arc::clone(registry),
    )
}

#[tokio::test]
async fn the_ban_sweep_ends_a_v4_socket_that_has_sent_no_position() {
    let registry = PeersRegistry::new();
    let peers = directory(gatekeeper().await, &registry);
    let (_banned, mut banned_events) =
        registry.on_v4_peer_connected("0xbanneduser", "0xs1", "conn-1");
    let (_clean, mut clean_events) = registry.on_v4_peer_connected("0xcleanuser", "0xs2", "conn-2");

    peers.ban_sweep_once().await;

    assert!(matches!(banned_events.try_recv(), Ok(SocketEvent::Kicked)));
    assert!(
        clean_events.try_recv().is_err(),
        "the clean wallet's socket is left alone"
    );
}

#[tokio::test]
async fn the_ban_sweep_ends_both_listeners_sockets_of_a_wallet_in_the_directory() {
    let registry = PeersRegistry::new();
    let peers = directory(gatekeeper().await, &registry);
    let (_legacy, mut legacy_events, _) = registry.on_peer_connected("0xbanneduser", "0xs1");
    let (_v4, mut v4_events) = registry.on_v4_peer_connected("0xbanneduser", "0xs2", "conn-1");
    peers.upsert_peer("0xbanneduser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    peers.ban_sweep_once().await;

    assert!(matches!(legacy_events.try_recv(), Ok(SocketEvent::Kicked)));
    assert!(matches!(v4_events.try_recv(), Ok(SocketEvent::Kicked)));
    assert!(peers.peer("0xbanneduser").is_none());
}
