//! Response-shape parity with upstream archipelago-workers/stats handlers.
//!
//! Upstream `peers-handler.ts` returns `{ ok, peers: [{id,address,lastPing,parcel,
//! position}] }`, mounted at BOTH `/peers` and `/comms/peers` (and `/{,comms/}peers/:id`).
//! Upstream `hot-scenes-handler.ts` returns a bare JSON array of HotSceneInfo (camelCase,
//! capped at 100, sorted by usersTotalCount desc). These tests pin the field names,
//! casing, status codes and the `?id=` filter against an in-process server.

use catalyrst_archipelago::config::{
    AuthConfig, ClusterConfig, Config, GossipConfig, LivekitConfig, ServerConfig,
};
use catalyrst_archipelago::{api_router, build_state, AppState};
use serde_json::Value;

fn test_config() -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        cluster: ClusterConfig::default(),
        server: ServerConfig::default(),
        auth: AuthConfig {
            // /peers and /hot-scenes are unauthenticated read endpoints; auth gates /ws.
            require_signed_challenge: false,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
        },
        livekit: LivekitConfig::default(),
        gossip: GossipConfig::default(),
        content_database_url: None,
        content_base_url: String::new(),
        commit_hash: "deadbeef".into(),
    }
}

async fn start_server() -> (u16, AppState) {
    let state = build_state(&test_config()).await.expect("state");
    let app = api_router().with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (port, state)
}

async fn get_json(port: u16, path: &str) -> (u16, Value) {
    let resp = reqwest::get(format!("http://127.0.0.1:{port}{path}"))
        .await
        .expect("request");
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.expect("json body");
    (status, body)
}

fn assert_peer_shape(peer: &Value, address: &str) {
    let obj = peer.as_object().expect("peer object");
    // Exact field set — upstream PeerResult is {id,address,lastPing,parcel,position}.
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["address", "id", "lastPing", "parcel", "position"]);
    assert_eq!(peer["id"], address);
    assert_eq!(peer["address"], address);
    assert!(peer["lastPing"].is_i64(), "lastPing must be a millis int");
    assert_eq!(peer["parcel"].as_array().unwrap().len(), 2);
    assert_eq!(peer["position"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn peers_shape_and_comms_alias_match() {
    let (port, state) = start_server().await;
    state.cluster.upsert_peer(
        "0xaaaa".into(),
        [16.0, 0.0, 32.0],
        [1, 2],
        "catalyrst".into(),
    );

    for path in ["/peers", "/comms/peers"] {
        let (status, body) = get_json(port, path).await;
        assert_eq!(status, 200, "{path}");
        assert_eq!(body["ok"], true, "{path} ok flag");
        let peers = body["peers"].as_array().expect("peers array");
        assert_eq!(peers.len(), 1, "{path}");
        assert_peer_shape(&peers[0], "0xaaaa");
        // toParcel(16,32) = floor(16/16),floor(32/16) = [1,2]
        assert_eq!(peers[0]["parcel"], serde_json::json!([1, 2]), "{path}");
        assert_eq!(
            peers[0]["position"],
            serde_json::json!([16.0, 0.0, 32.0]),
            "{path}"
        );
    }
}

#[tokio::test]
async fn peers_id_filter_is_case_insensitive() {
    let (port, state) = start_server().await;
    state
        .cluster
        .upsert_peer("0xaaaa".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());
    state
        .cluster
        .upsert_peer("0xbbbb".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    // Mixed-case (EIP-55 style) id must still match the lowercased store.
    let (status, body) = get_json(port, "/peers?id=0xAAAA").await;
    assert_eq!(status, 200);
    let peers = body["peers"].as_array().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0]["address"], "0xaaaa");
}

#[tokio::test]
async fn peer_by_id_found_and_not_found() {
    let (port, state) = start_server().await;
    state
        .cluster
        .upsert_peer("0xaaaa".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    for path in ["/peers/0xaaaa", "/comms/peers/0xAAAA"] {
        let (status, body) = get_json(port, path).await;
        assert_eq!(status, 200, "{path}");
        assert_eq!(body["ok"], true, "{path}");
        assert_peer_shape(&body["peer"], "0xaaaa");
    }

    let (status, body) = get_json(port, "/peers/0xdoesnotexist").await;
    assert_eq!(status, 404);
    assert_eq!(body["ok"], false);
    assert!(body["peer"].is_null());
}

#[tokio::test]
async fn hot_scenes_is_json_array() {
    // With no content DB wired, fetch_scenes yields empty -> a bare `[]` array,
    // matching upstream's array-shaped (not object-wrapped) response and 200 status.
    let (port, state) = start_server().await;
    state
        .cluster
        .upsert_peer("0xaaaa".into(), [0.0, 0.0, 0.0], [0, 0], "r".into());

    let (status, body) = get_json(port, "/hot-scenes").await;
    assert_eq!(status, 200);
    assert!(body.is_array(), "hot-scenes body must be a JSON array");
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn status_uses_camelcase_fields() {
    let (port, _state) = start_server().await;
    let (status, body) = get_json(port, "/status").await;
    assert_eq!(status, 200);
    let obj = body.as_object().unwrap();
    // Upstream status-handler.ts: { version, currentTime, commitHash }.
    assert!(obj.contains_key("version"));
    assert!(obj.contains_key("currentTime"));
    assert_eq!(body["commitHash"], "deadbeef");
}
