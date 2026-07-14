use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::messaging::create_group;
use catalyrst_comms::ports::names::NamesComponent;
use catalyrst_comms::ports::player_connection::PlayerConnectionComponent;
use catalyrst_comms::ports::player_reports::PlayerReportsComponent;
use catalyrst_comms::ports::scene_admin::SceneAdminComponent;
use catalyrst_comms::ports::scene_bans::SceneBansComponent;
use catalyrst_comms::ports::user_bans::UserBansComponent;
use catalyrst_comms::voice_db::{VoiceDb, VoiceDbConfig};
use catalyrst_comms::{AppState, AppStateInner};
use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_crypto::{create_simple_auth_chain, Wallet};
use serde_json::json;
use sqlx::PgPool;

const ATTACKER_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const VICTIM_COMMUNITY: &str = "5f6d4c3b-2a19-4e88-9f00-000000000042";
const SERVICE_TOKEN: &str = "gatekeeper-service-token";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_mlsgroup").await?;
    scratch
        .apply_sql(include_str!("../migrations/0004_mls_messaging.sql"))
        .await;
    Some(scratch)
}

fn test_state(pool: PgPool, gatekeeper_auth_token: Option<String>) -> AppState {
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

fn signed_headers(wallet: &Wallet, method: &str, path: &str) -> HeaderMap {
    let timestamp = chrono::Utc::now().timestamp_millis().to_string();
    let payload = build_payload(method, path, &timestamp, "{}");
    let chain = create_simple_auth_chain(wallet, &payload).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp).unwrap(),
    );
    headers.insert(AUTH_METADATA_HEADER, HeaderValue::from_static("{}"));
    for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
        headers.insert(
            HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes()).unwrap(),
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
    }
    headers
}

fn fresh_group_id() -> String {
    format!("{:064x}", uuid::Uuid::new_v4().as_u128())
}

async fn stored_community_id(pool: &PgPool, group_id: &str) -> Option<Option<String>> {
    sqlx::query_scalar("SELECT community_id FROM mls_groups WHERE group_id = $1")
        .bind(group_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn body_community_id_is_not_stamped_on_a_freshly_minted_group() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let state = test_state(pool.clone(), Some(SERVICE_TOKEN.into()));

    let attacker = Wallet::from_hex(ATTACKER_KEY).unwrap();
    let group_id = fresh_group_id();
    let body = json!({
        "group_id": group_id,
        "group_kind": "channel",
        "community_id": VICTIM_COMMUNITY,
        "initial_members": [attacker.address().to_lowercase()],
    });

    let result = create_group(
        State(state),
        signed_headers(&attacker, "post", "/mls/groups"),
        Bytes::from(body.to_string()),
    )
    .await;

    let stored = stored_community_id(&pool, &group_id).await;
    assert_ne!(
        stored.clone().flatten().as_deref(),
        Some(VICTIM_COMMUNITY),
        "a wallet with no proven relationship to community {VICTIM_COMMUNITY} stamped a group with it"
    );
    assert!(
        stored.is_none(),
        "the unauthorized community binding must not produce a group row at all"
    );
    let err = result.expect_err("the request must be rejected");
    assert_eq!(err.code, 403, "unexpected status: {}", err.message);

    scratch.drop().await;
}

#[tokio::test]
async fn service_token_may_bind_a_community_channel() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let state = test_state(pool.clone(), Some(SERVICE_TOKEN.into()));

    let creator = Wallet::from_hex(ATTACKER_KEY).unwrap();
    let group_id = fresh_group_id();
    let body = json!({
        "group_id": group_id,
        "group_kind": "channel",
        "community_id": VICTIM_COMMUNITY,
        "initial_members": [creator.address().to_lowercase()],
    });

    let mut headers = signed_headers(&creator, "post", "/mls/groups");
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {SERVICE_TOKEN}")).unwrap(),
    );

    let _ = create_group(State(state), headers, Bytes::from(body.to_string()))
        .await
        .expect("the platform service must still be able to create community channels");

    assert_eq!(
        stored_community_id(&pool, &group_id).await.flatten(),
        Some(VICTIM_COMMUNITY.to_string())
    );

    scratch.drop().await;
}

#[tokio::test]
async fn plain_dm_group_creation_is_unaffected() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    let state = test_state(pool.clone(), None);

    let creator = Wallet::from_hex(ATTACKER_KEY).unwrap();
    let group_id = fresh_group_id();
    let body = json!({
        "group_id": group_id,
        "group_kind": "dm",
        "initial_members": [
            creator.address().to_lowercase(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ],
    });

    let _ = create_group(
        State(state),
        signed_headers(&creator, "post", "/mls/groups"),
        Bytes::from(body.to_string()),
    )
    .await
    .expect("a plain dm must still be creatable by a signed wallet");

    assert_eq!(stored_community_id(&pool, &group_id).await, Some(None));

    scratch.drop().await;
}
