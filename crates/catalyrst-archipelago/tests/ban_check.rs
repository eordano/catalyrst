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
use catalyrst_archipelago::config::{ClusterConfig, LivekitConfig};
use catalyrst_archipelago::livekit::LivekitMinter;
use catalyrst_archipelago::peers::PeerDirectory;
use catalyrst_archipelago::registry::{PeersRegistry, SocketEvent};

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

async fn bans_500_counted(State(c): State<Arc<Counters>>) -> StatusCode {
    c.ban_hits.fetch_add(1, Ordering::SeqCst);
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
    let mut users = vec![json!({ "wallet": "0xBAD0000000000000000000000000000000000bad" })];
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
        .route("/counted-bad/users/{addr}/bans", get(bans_500_counted))
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
    assert!(
        deny.is_denied("0xBAD0000000000000000000000000000000000BAD")
            .await
    );
    assert!(
        deny.is_denied("0xbad0000000000000000000000000000000000bad")
            .await
    );
    assert!(
        !deny
            .is_denied("0x1111111111111111111111111111111111111111")
            .await
    );
    assert!(
        deny.is_denied("0xsomeoneelse").await,
        "malformed addresses must be denied, not silently allowed"
    );
}

#[tokio::test]
async fn deny_list_caches_within_ttl() {
    let (port, counters) = start_mock().await;
    let deny = DenyList::new(
        Some(format!("http://127.0.0.1:{port}/denylist.json")),
        reqwest::Client::new(),
    );
    for who in [
        "0x1111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222",
        "0x3333333333333333333333333333333333333333",
    ] {
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
    assert!(
        deny.is_denied("0xbad0000000000000000000000000000000000bad")
            .await
    );
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(
        deny.is_denied("0xbad0000000000000000000000000000000000bad")
            .await
    );
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
async fn the_ban_sweep_evicts_the_banned_peer_and_leaves_the_clean_one() {
    let (port, _c) = start_mock().await;
    let ban = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}")),
        reqwest::Client::new(),
    );
    let registry = PeersRegistry::new();
    let peers = PeerDirectory::new(
        ClusterConfig::default(),
        armed_minter(),
        ban,
        Arc::clone(&registry),
    );
    let (_banned_link, mut banned_events, _) = registry.on_peer_connected("0xbanneduser", "0xs1");
    let (_clean_link, mut clean_events, _) = registry.on_peer_connected("0xcleanuser", "0xs2");

    peers.upsert_peer("0xbanneduser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
    peers.upsert_peer("0xcleanuser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    peers.ban_sweep_once().await;

    assert!(peers.peer("0xbanneduser").is_none(), "banned peer evicted");
    assert_eq!(peers.peers_count(), 1);
    assert!(peers.peer("0xcleanuser").is_some(), "clean peer survives");
    assert!(matches!(banned_events.try_recv(), Ok(SocketEvent::Kicked)));
    assert!(
        clean_events.try_recv().is_err(),
        "the clean peer's socket is left alone"
    );
}

#[tokio::test]
async fn the_ban_sweep_with_a_disarmed_checker_keeps_everyone() {
    let ban = BanChecker::new(None, reqwest::Client::new());
    let peers = PeerDirectory::new(
        ClusterConfig::default(),
        armed_minter(),
        ban,
        PeersRegistry::new(),
    );
    peers.upsert_peer("0xbanneduser".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
    peers.ban_sweep_once().await;
    assert!(peers.peer("0xbanneduser").is_some());
    assert_eq!(peers.peers_count(), 1);
}

mod ws_deny {
    use super::*;
    use alloy::signers::{local::PrivateKeySigner, SignerSync};
    use catalyrst_archipelago::config::{AuthConfig, Config, NatsConfig, ServerConfig};
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
                ..AuthConfig::default()
            },
            livekit: LivekitConfig::default(),
            nats: NatsConfig::default(),
            content_database_url: None,
            control_database_url: None,
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
        ClientPacket {
            request_sequence: 0,
            message: Some(msg),
        }
        .encode_to_vec()
        .into()
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
                offer: None,
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
                typed_auth_chain: None,
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

#[tokio::test]
async fn ban_checker_memoizes_verdicts_and_a_fresh_check_refreshes_them() {
    let (port, c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}")),
        reqwest::Client::new(),
    );
    assert!(!checker.is_banned("0xcleanuser").await);
    assert!(!checker.is_banned("0xcleanuser").await);
    assert_eq!(
        c.ban_hits.load(Ordering::SeqCst),
        1,
        "second check is served from the memo"
    );
    assert!(!checker.is_banned_fresh("0xcleanuser").await);
    assert_eq!(
        c.ban_hits.load(Ordering::SeqCst),
        2,
        "the sweep always asks"
    );
    assert!(checker.is_banned("0xbanneduser").await);
    assert!(checker.is_banned("0xbanneduser").await);
    assert_eq!(c.ban_hits.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn ban_checker_does_not_memoize_a_lookup_that_failed_open() {
    let (port, c) = start_mock().await;
    let checker = BanChecker::new(
        Some(format!("http://127.0.0.1:{port}/counted-bad")),
        reqwest::Client::new(),
    );
    assert!(!checker.is_banned("0xbanneduser").await);
    assert!(!checker.is_banned("0xbanneduser").await);
    assert_eq!(
        c.ban_hits.load(Ordering::SeqCst),
        2,
        "a failed lookup is asked again"
    );
}
