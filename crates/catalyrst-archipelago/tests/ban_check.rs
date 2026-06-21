use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use catalyrst_archipelago::ban::{BanChecker, DenyList};
use catalyrst_archipelago::cluster::{Cluster, ClusterEvent};
use catalyrst_archipelago::config::{ClusterConfig, LivekitConfig};
use catalyrst_archipelago::livekit::LivekitMinter;

#[derive(Default)]
struct Counters {
    ban_hits: AtomicUsize,
    denylist_hits: AtomicUsize,
}

async fn bans_ok(State(c): State<Arc<Counters>>, Path(addr): Path<String>) -> Json<Value> {
    c.ban_hits.fetch_add(1, Ordering::SeqCst);
    let is_banned = addr.to_ascii_lowercase().contains("banned");
    Json(json!({ "data": { "isBanned": is_banned } }))
}

async fn bans_500() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn bans_malformed() -> &'static str {
    "this is not json"
}

async fn bans_missing() -> Json<Value> {
    Json(json!({ "data": {} }))
}

async fn denylist(
    State(c): State<Arc<Counters>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Value> {
    c.denylist_hits.fetch_add(1, Ordering::SeqCst);
    let mut users = vec![json!({ "wallet": "0xBANNEDWALLET" })];
    if let Some(w) = params.get("wallet") {
        users.push(json!({ "wallet": w }));
    }
    Json(json!({ "users": users }))
}

async fn start_mock() -> (u16, Arc<Counters>) {
    let counters = Arc::new(Counters::default());
    let app = Router::new()
        .route("/users/{addr}/bans", get(bans_ok))
        .route("/bad/users/{addr}/bans", get(bans_500))
        .route("/malformed/users/{addr}/bans", get(bans_malformed))
        .route("/missing/users/{addr}/bans", get(bans_missing))
        .route("/denylist.json", get(denylist))
        .with_state(counters.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, counters)
}

#[tokio::test]
async fn ban_checker_flags_banned_and_allows_others() {
    let (port, _c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}")),
        reqwest::Client::new(),
    );
    assert!(checker.is_armed());
    assert!(checker.is_banned("0xBANNEDuser").await);
    assert!(!checker.is_banned("0xcleanuser").await);
}

#[tokio::test]
async fn ban_checker_fails_open_on_non_ok_status() {
    let (port, _c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}/bad")),
        reqwest::Client::new(),
    );
    assert!(!checker.is_banned("0xbanneduser").await);
}

#[tokio::test]
async fn ban_checker_fails_open_on_malformed_body() {
    let (port, _c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}/malformed")),
        reqwest::Client::new(),
    );
    assert!(!checker.is_banned("0xbanneduser").await);
}

#[tokio::test]
async fn ban_checker_treats_missing_field_as_not_banned() {
    let (port, _c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}/missing")),
        reqwest::Client::new(),
    );
    assert!(!checker.is_banned("0xbanneduser").await);
}

#[tokio::test]
async fn ban_checker_fails_open_when_gatekeeper_unreachable() {
    let checker = BanChecker::new(Some("http://127.0.0.1:1".into()), reqwest::Client::new());
    assert!(!checker.is_banned("0xbanneduser").await);
}

#[tokio::test]
async fn deny_list_flags_listed_wallet_case_insensitively() {
    let (port, _c) = start_mock().await;
    let deny = DenyList::new(
        Some(format!("http://127.0.0.1:{port}/denylist.json")),
        reqwest::Client::new(),
    );
    assert!(deny.is_armed());
    assert!(deny.is_denied("0xBANNEDWALLET").await);
    assert!(deny.is_denied("0xbannedwallet").await);
    assert!(!deny.is_denied("0xsomeoneelse").await);
}

#[tokio::test]
async fn deny_list_caches_within_ttl() {
    let (port, counters) = start_mock().await;
    let deny = DenyList::new(
        Some(format!("http://127.0.0.1:{port}/denylist.json")),
        reqwest::Client::new(),
    );
    for who in ["0xa", "0xb", "0xc"] {
        deny.is_denied(who).await;
    }
    assert_eq!(
        counters.denylist_hits.load(Ordering::SeqCst),
        1,
        "the 5-minute TTL must collapse repeated lookups into one fetch"
    );
}

#[tokio::test]
async fn deny_list_refetches_after_ttl_and_fails_open_on_outage() {
    let (port, counters) = start_mock().await;
    let deny = DenyList::with_ttl(
        Some(format!("http://127.0.0.1:{port}/denylist.json")),
        reqwest::Client::new(),
        Duration::from_millis(0),
    );
    assert!(deny.is_denied("0xbannedwallet").await);
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(deny.is_denied("0xbannedwallet").await);
    assert_eq!(counters.denylist_hits.load(Ordering::SeqCst), 2);
}

fn armed_minter() -> Arc<LivekitMinter> {
    Arc::new(LivekitMinter::new(LivekitConfig {
        api_key: Some("APIabc".into()),
        api_secret: Some("supersecret".into()),
        ws_url: "wss://lk.example".into(),
        token_ttl_secs: 60,
        comms_gatekeeper_url: None,
    }))
}

#[tokio::test]
async fn recluster_evicts_banned_peer_and_assigns_clean_one() {
    let (port, _c) = start_mock().await;
    let ban = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}")),
        reqwest::Client::new(),
    );
    let cluster = Cluster::new(ClusterConfig::default(), armed_minter(), ban);

    let mut rx = cluster.subscribe();
    cluster.upsert_peer("0xbanneduser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
    cluster.upsert_peer("0xcleanuser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    cluster.recluster_once().await;

    assert!(
        cluster.peer("0xbanneduser").is_none(),
        "banned peer evicted"
    );
    assert_eq!(cluster.peers_count(), 1);
    let clean = cluster.peer("0xcleanuser").expect("clean peer survives");
    assert!(clean.island_id.is_some(), "clean peer keeps an island");

    let mut island_changed_addrs = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let ClusterEvent::IslandChanged { address, .. } = evt {
            island_changed_addrs.push(address);
        }
    }
    assert_eq!(island_changed_addrs, vec!["0xcleanuser".to_string()]);
}

#[tokio::test]
async fn recluster_with_disarmed_ban_checker_keeps_everyone() {
    let ban = BanChecker::new(None, reqwest::Client::new());
    let cluster = Cluster::new(ClusterConfig::default(), armed_minter(), ban);
    cluster.upsert_peer("0xbanneduser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
    cluster.recluster_once().await;
    assert!(cluster.peer("0xbanneduser").is_some());
    assert_eq!(cluster.peers_count(), 1);
}

mod ws_deny {
    use super::*;
    use alloy::signers::{local::PrivateKeySigner, SignerSync};
    use catalyrst_archipelago::config::{AuthConfig, Config, GossipConfig, ServerConfig};
    use catalyrst_archipelago::proto::archipelago::{
        client_packet, server_packet, ChallengeRequestMessage, ClientPacket, ServerPacket,
        SignedChallengeMessage,
    };
    use catalyrst_archipelago::{api_router, build_state};
    use futures::{SinkExt, StreamExt};
    use prost::Message as _;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    type WsStream = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    fn config_with_deny_list(deny_list_url: Option<String>) -> Config {
        Config {
            http_host: "127.0.0.1".into(),
            http_port: 0,
            cluster: ClusterConfig::default(),
            server: ServerConfig::default(),
            auth: AuthConfig {
                require_signed_challenge: true,
                challenge_ttl_secs: 120,
                signature_max_age_secs: 300,
                deny_list_url,
            },
            livekit: LivekitConfig::default(),
            gossip: GossipConfig::default(),
            content_database_url: None,
            content_base_url: String::new(),
            commit_hash: String::new(),
        }
    }

    async fn start_archipelago(deny_list_url: Option<String>) -> u16 {
        let state = build_state(&config_with_deny_list(deny_list_url))
            .await
            .expect("state");
        let app = api_router().with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        port
    }

    fn encode(msg: client_packet::Message) -> tokio_tungstenite::tungstenite::Bytes {
        ClientPacket { message: Some(msg) }.encode_to_vec().into()
    }

    async fn recv_msg(ws: &mut WsStream, timeout: Duration) -> Option<server_packet::Message> {
        loop {
            let frame = tokio::time::timeout(timeout, ws.next()).await.ok()??;
            match frame.ok()? {
                WsMessage::Binary(bytes) => {
                    return ServerPacket::decode(bytes.as_ref()).ok()?.message;
                }
                WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                _ => return None,
            }
        }
    }

    async fn try_handshake(port: u16, wallet: &PrivateKeySigner) -> Option<()> {
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .expect("ws connect");
        let address = format!("{:#x}", wallet.address());
        ws.send(WsMessage::Binary(encode(
            client_packet::Message::ChallengeRequest(ChallengeRequestMessage {
                address: address.clone(),
            }),
        )))
        .await
        .expect("send challenge request");

        let challenge = match recv_msg(&mut ws, Duration::from_secs(3)).await {
            Some(server_packet::Message::ChallengeResponse(r)) => r.challenge_to_sign,
            _ => return None,
        };

        let hash = alloy::primitives::eip191_hash_message(challenge.as_bytes());
        let sig = wallet.sign_hash_sync(&hash).expect("sign");
        let chain = json!([
            { "type": "SIGNER", "payload": address, "signature": "" },
            { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge, "signature": sig.to_string() }
        ]);
        ws.send(WsMessage::Binary(encode(
            client_packet::Message::SignedChallenge(SignedChallengeMessage {
                auth_chain_json: chain.to_string(),
            }),
        )))
        .await
        .expect("send signed challenge");

        matches!(
            recv_msg(&mut ws, Duration::from_secs(3)).await,
            Some(server_packet::Message::Welcome(_))
        )
        .then_some(())
    }

    #[tokio::test]
    async fn deny_listed_wallet_is_rejected_after_auth() {
        let (mock_port, _c) = start_mock().await;
        let wallet = PrivateKeySigner::random();
        let addr = format!("{:#x}", wallet.address());
        let deny_url = format!("http://127.0.0.1:{mock_port}/denylist.json?wallet={addr}");
        let port = start_archipelago(Some(deny_url)).await;

        assert!(
            try_handshake(port, &wallet).await.is_none(),
            "a deny-listed wallet must not receive a Welcome"
        );
    }

    #[tokio::test]
    async fn non_denied_wallet_completes_handshake() {
        let (mock_port, _c) = start_mock().await;
        let wallet = PrivateKeySigner::random();
        let deny_url = format!("http://127.0.0.1:{mock_port}/denylist.json?wallet=0xsomeoneelse");
        let port = start_archipelago(Some(deny_url)).await;

        assert!(
            try_handshake(port, &wallet).await.is_some(),
            "a non-denied wallet must complete the handshake"
        );
    }
}
