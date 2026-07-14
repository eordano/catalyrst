use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{api_router, AppState, AppStateInner};
use serde_json::json;
use sqlx::PgPool;

const SERVICE_TOKEN: &str = "gatekeeper-service-token";
const WORLD_BAN_STATUS: &str =
    "/worlds/someworld.dcl.eth/parcels/0,0/users/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/ban-status";

fn test_state(gatekeeper_auth_token: Option<String>) -> AppState {
    let pool = PgPool::connect_lazy("postgres://postgres@127.0.0.1:1/postgres")
        .expect("lazy pool never connects in this test");
    Arc::new(AppStateInner {
        scene_admin: SceneAdminComponent::new(pool.clone()),
        scene_bans: SceneBansComponent::new(pool.clone()),
        user_bans: UserBansComponent::new(pool.clone()),
        player_connection: PlayerConnectionComponent::new(pool.clone()),
        player_reports: PlayerReportsComponent::new(pool.clone()),
        names: NamesComponent::new(None, "squid_marketplace".into()),
        voice_db: VoiceDb::new(pool.clone(), VoiceDbConfig::from_env()),
        places_pool: None,
        dapps_pool: None,
        dapps_schema: "squid_marketplace".into(),
        http: reqwest::Client::new(),
        catalyst_url: "http://127.0.0.1:1".into(),
        world_content_url: "http://127.0.0.1:1".into(),
        lambdas_url: "http://127.0.0.1:1".into(),
        pool,
        livekit_host: "livekit.local".into(),
        livekit_ws_url: "wss://livekit.local".into(),
        livekit_api_key: "devkey".into(),
        livekit_api_secret: "devsecret".into(),
        livekit_webhook_key: None,
        livekit_configured: true,
        private_messages_room_id: "private-messages".into(),
        authoritative_server_address: None,
        moderator_token: None,
        moderator_addresses: Vec::new(),
        gatekeeper_auth_token,
        fed_peer_id: "test-peer".into(),
    })
}

async fn serve(gatekeeper_auth_token: Option<String>) -> SocketAddr {
    let state = test_state(gatekeeper_auth_token);
    let app = Router::new()
        .merge(api_router(state.clone()))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn unconfigured_service_token_refuses_the_bearer_gated_voice_routes() {
    let addr = serve(None).await;

    let res = reqwest::Client::new()
        .post(format!("http://{addr}/community-voice-chat"))
        .json(&json!({
            "community_id": "",
            "user_address": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        }))
        .send()
        .await
        .unwrap();

    let status = res.status().as_u16();
    let body = res.text().await.unwrap();
    assert_eq!(
        status, 503,
        "an unauthenticated caller reached the voice handler because COMMS_GATEKEEPER_AUTH_TOKEN is unset (status {status}, body {body})"
    );
}

#[tokio::test]
async fn unconfigured_service_token_refuses_the_world_ban_status_route() {
    let addr = serve(None).await;

    let res = reqwest::Client::new()
        .get(format!("http://{addr}{WORLD_BAN_STATUS}"))
        .send()
        .await
        .unwrap();

    let status = res.status().as_u16();
    let body = res.text().await.unwrap();
    assert_eq!(
        status, 503,
        "an unauthenticated caller read world ban status because COMMS_GATEKEEPER_AUTH_TOKEN is unset (status {status}, body {body})"
    );
}

#[tokio::test]
async fn configured_service_token_still_gates_on_the_bearer() {
    let addr = serve(Some(SERVICE_TOKEN.into())).await;
    let client = reqwest::Client::new();

    let missing = client
        .post(format!("http://{addr}/community-voice-chat"))
        .json(&json!({ "community_id": "", "user_address": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status().as_u16(), 401);

    let wrong = client
        .post(format!("http://{addr}/community-voice-chat"))
        .header("authorization", "Bearer not-the-token")
        .json(&json!({ "community_id": "", "user_address": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status().as_u16(), 401);

    let accepted = client
        .post(format!("http://{addr}/community-voice-chat"))
        .header("authorization", format!("Bearer {SERVICE_TOKEN}"))
        .json(&json!({ "community_id": "", "user_address": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        accepted.status().as_u16(),
        400,
        "the platform service must still reach the handler"
    );
}

#[tokio::test]
async fn ungated_routes_are_untouched_by_the_fail_closed_gate() {
    let addr = serve(None).await;

    let res = reqwest::Client::new()
        .post(format!("http://{addr}/get-scene-adapter"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status().as_u16(),
        400,
        "an unsigned scene-adapter request must still be answered by its own auth check"
    );
}
