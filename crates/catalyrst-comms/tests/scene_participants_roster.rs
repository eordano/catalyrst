use std::sync::Arc;

use axum::extract::{Query, State};
use catalyrst_comms::handlers::scene_bans::resolve_listing_place_id;
use catalyrst_comms::handlers::scene_participants::{list_participants, ParticipantsQuery};
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use sqlx::postgres::PgPoolOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn mock_http(
    status_line: &'static str,
    resp_body: &'static str,
) -> (
    String,
    tokio::sync::oneshot::Receiver<(String, serde_json::Value)>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 16384];
        let mut total = 0;
        loop {
            let n = sock.read(&mut buf[total..]).await.unwrap();
            if n == 0 {
                break;
            }
            total += n;
            let text = String::from_utf8_lossy(&buf[..total]);
            if let Some(hdr_end) = text.find("\r\n\r\n") {
                let content_len = text[..hdr_end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                if total >= hdr_end + 4 + content_len {
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&buf[..total]).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let line = head.lines().next().unwrap_or("").to_string();
        let body_json: serde_json::Value =
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        let resp = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{resp_body}",
            resp_body.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
        sock.flush().await.unwrap();
        let _ = tx.send((line, body_json));
    });
    (format!("http://{addr}"), rx)
}

fn lazy_state(catalyst_url: &str, world_content_url: &str, livekit_host: &str) -> AppState {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .unwrap();
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool: None,
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: catalyst_url.trim_end_matches('/').to_string(),
        world_content_url: world_content_url.trim_end_matches('/').to_string(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_host: livekit_host.to_string(),
        livekit_ws_url: "wss://livekit.example.com".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        livekit_token_ttl_secs: 600,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token: None,
    })
}

fn query(pointer: Option<&str>, realm_name: Option<&str>) -> Query<ParticipantsQuery> {
    Query(ParticipantsQuery {
        pointer: pointer.map(String::from),
        realm_name: realm_name.map(String::from),
        room: None,
    })
}

#[tokio::test]
async fn world_pointer_targets_world_scene_room_via_worlds_server() {
    let (worlds_url, worlds_rx) = mock_http(
        "200 OK",
        r#"{"scenes":[{"entityId":"bafkreiwscene"}],"total":1}"#,
    )
    .await;
    let (livekit_url, livekit_rx) = mock_http(
        "200 OK",
        r#"{"participants":[{"identity":"0x1234567890ABCDEF1234567890ABCDEF12345678:peer"},{"identity":"authoritative-server"}]}"#,
    )
    .await;
    let state = lazy_state("http://127.0.0.1:1", &worlds_url, &livekit_url);

    let resp = list_participants(State(state), query(Some("0,0"), Some("foo.eth")))
        .await
        .expect("roster");
    assert_eq!(
        resp.0,
        serde_json::json!({
            "ok": true,
            "data": { "addresses": ["0x1234567890abcdef1234567890abcdef12345678"] }
        })
    );

    let (worlds_line, worlds_body) = worlds_rx.await.unwrap();
    assert_eq!(worlds_line, "POST /world/foo.eth/scenes HTTP/1.1");
    assert_eq!(worlds_body, serde_json::json!({ "pointers": ["0,0"] }));

    let (livekit_line, livekit_body) = livekit_rx.await.unwrap();
    assert_eq!(
        livekit_line,
        "POST /twirp/livekit.RoomService/ListParticipants HTTP/1.1"
    );
    assert_eq!(
        livekit_body,
        serde_json::json!({ "room": "world-foo.eth-bafkreiwscene" })
    );
}

#[tokio::test]
async fn pointer_without_realm_defaults_to_main_scene_room() {
    let (catalyst_url, catalyst_rx) = mock_http("200 OK", r#"[{"id":"bafkreigenesis"}]"#).await;
    let (livekit_url, livekit_rx) = mock_http("200 OK", r#"{"participants":[]}"#).await;
    let state = lazy_state(&catalyst_url, "http://127.0.0.1:1", &livekit_url);

    let resp = list_participants(State(state), query(Some("-7,-2"), None))
        .await
        .expect("roster");
    assert_eq!(
        resp.0,
        serde_json::json!({ "ok": true, "data": { "addresses": [] } })
    );

    let (catalyst_line, catalyst_body) = catalyst_rx.await.unwrap();
    assert_eq!(catalyst_line, "POST /content/entities/active HTTP/1.1");
    assert_eq!(catalyst_body, serde_json::json!({ "pointers": ["-7,-2"] }));

    let (_, livekit_body) = livekit_rx.await.unwrap();
    assert_eq!(
        livekit_body,
        serde_json::json!({ "room": "scene:bafkreigenesis" })
    );
}

#[tokio::test]
async fn world_without_pointer_targets_world_room() {
    let (livekit_url, livekit_rx) = mock_http("200 OK", r#"{"participants":[]}"#).await;
    let state = lazy_state("http://127.0.0.1:1", "http://127.0.0.1:1", &livekit_url);

    let resp = list_participants(State(state), query(None, Some("bar.dcl.eth")))
        .await
        .expect("roster");
    assert_eq!(
        resp.0,
        serde_json::json!({ "ok": true, "data": { "addresses": [] } })
    );

    let (_, livekit_body) = livekit_rx.await.unwrap();
    assert_eq!(
        livekit_body,
        serde_json::json!({ "room": "world-bar.dcl.eth" })
    );
}

#[tokio::test]
async fn missing_pointer_and_realm_is_400() {
    let state = lazy_state("http://127.0.0.1:1", "http://127.0.0.1:1", "");
    let err = list_participants(State(state), query(None, None))
        .await
        .expect_err("must reject");
    assert_eq!(err.code, 400);
    assert_eq!(err.message, "Either pointer or realm_name must be provided");
}

#[tokio::test]
async fn unresolved_pointer_is_404() {
    let (catalyst_url, _rx) = mock_http("200 OK", "[]").await;
    let state = lazy_state(&catalyst_url, "http://127.0.0.1:1", "");
    let err = list_participants(State(state), query(Some("-7,-2"), Some("main")))
        .await
        .expect_err("must 404");
    assert_eq!(err.code, 404);
    assert_eq!(err.message, "No scene found for pointer: -7,-2");
}

#[tokio::test]
async fn unresolved_world_pointer_is_404() {
    let (worlds_url, _rx) = mock_http("200 OK", r#"{"scenes":[],"total":0}"#).await;
    let state = lazy_state("http://127.0.0.1:1", &worlds_url, "");
    let err = list_participants(State(state), query(Some("9,9"), Some("foo.eth")))
        .await
        .expect_err("must 404");
    assert_eq!(err.code, 404);
    assert_eq!(
        err.message,
        "No scene found for world foo.eth at pointer: 9,9"
    );
}

#[tokio::test]
async fn listing_place_id_resolves_world_metadata_like_the_hot_path() {
    let (worlds_url, worlds_rx) = mock_http(
        "200 OK",
        r#"{"configurations":{"scenesUrn":["urn:decentraland:entity:bafkreiworld?baseUrl=https://x/contents/"]}}"#,
    )
    .await;
    let state = lazy_state("http://127.0.0.1:1", &worlds_url, "");
    let meta = serde_json::json!({ "realmName": "Foo.dcl.eth" });
    let place_id = resolve_listing_place_id(&state, None, &meta)
        .await
        .expect("resolved");
    assert_eq!(place_id, "bafkreiworld");
    let (line, _) = worlds_rx.await.unwrap();
    assert_eq!(line, "GET /world/foo.dcl.eth/about HTTP/1.1");
}

#[tokio::test]
async fn listing_place_id_resolves_explicit_world_names_too() {
    let (worlds_url, _rx) = mock_http(
        "200 OK",
        r#"{"configurations":{"scenesUrn":["urn:decentraland:entity:bafkreiexplicit"]}}"#,
    )
    .await;
    let state = lazy_state("http://127.0.0.1:1", &worlds_url, "");
    let place_id =
        resolve_listing_place_id(&state, Some("foo.dcl.eth".into()), &serde_json::json!({}))
            .await
            .expect("resolved");
    assert_eq!(place_id, "bafkreiexplicit");
}

#[tokio::test]
async fn listing_place_id_prefers_metadata_scene_hash_without_network() {
    let state = lazy_state("http://127.0.0.1:1", "http://127.0.0.1:1", "");
    let meta = serde_json::json!({ "realmName": "foo.dcl.eth", "sceneId": "bafkreidirect" });
    let place_id = resolve_listing_place_id(&state, None, &meta)
        .await
        .expect("no network needed");
    assert_eq!(place_id, "bafkreidirect");

    let explicit = resolve_listing_place_id(&state, Some("bafkreiquery".into()), &meta)
        .await
        .expect("explicit hash passthrough");
    assert_eq!(explicit, "bafkreiquery");
}

#[tokio::test]
async fn listing_place_id_errors_when_world_resolution_fails() {
    let state = lazy_state("http://127.0.0.1:1", "http://127.0.0.1:1", "");
    let err = resolve_listing_place_id(
        &state,
        None,
        &serde_json::json!({ "realmName": "gone.dcl.eth" }),
    )
    .await
    .expect_err("unresolvable world must error");
    assert_eq!(err.code, 400);
    assert_eq!(
        err.message,
        "Failed to resolve scene ID for world gone.dcl.eth"
    );
}

#[tokio::test]
async fn listing_place_id_requires_some_key() {
    let state = lazy_state("http://127.0.0.1:1", "http://127.0.0.1:1", "");
    let err = resolve_listing_place_id(&state, None, &serde_json::json!({ "realmName": "main" }))
        .await
        .expect_err("no key");
    assert_eq!(err.code, 400);
    assert_eq!(err.message, "missing place_id query");
}
