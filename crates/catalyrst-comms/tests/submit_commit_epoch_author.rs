//! Regression test for the MLS epoch-author federation guard in `submit_commit`.
//!
//! A group's `epoch_author` (the `fed_peer_id` of the catalyst that created it)
//! is the single instance allowed to advance the group's epoch. A commit
//! submitted to any OTHER catalyst must be refused with `409 wrong epoch author`
//! and told where to send it -- otherwise two federated instances could fork the
//! epoch. This pins that cross-instance refusal.
//!
//! DB-gated via the shared scratch cluster (`CATALYRST_COMMS_TEST_PG` /
//! `CATALYRST_TEST_PG`); skips cleanly under `ALLOW_SKIPPED_INTEGRATION=1`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_comms::auth_chain::{
    build_payload, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
};
use catalyrst_comms::handlers::messaging::{create_group, submit_commit};
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

// Well-known hardhat account #1 private key -- a valid secp256k1 key.
const CREATOR_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_epochauthor").await?;
    scratch
        .apply_sql(include_str!("../migrations/0004_mls_messaging.sql"))
        .await;
    Some(scratch)
}

/// An `AppState` over `pool` with an explicit federation identity -- the axis this
/// test varies to model two distinct catalysts sharing one delivery-service DB.
fn test_state(pool: PgPool, fed_peer_id: &str) -> AppState {
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
        gatekeeper_auth_token: None,
        fed_peer_id: fed_peer_id.into(),
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

#[tokio::test]
async fn submit_commit_from_non_epoch_author_catalyst_is_409() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();

    let epoch_author = test_state(pool.clone(), "peer-a");
    let other_catalyst = test_state(pool.clone(), "peer-b");

    let creator = Wallet::from_hex(CREATOR_KEY).unwrap();
    let group_id = fresh_group_id();

    let body = json!({
        "group_id": group_id,
        "group_kind": "channel",
        "initial_members": [creator.address().to_lowercase()],
    });
    let _ = create_group(
        State(epoch_author),
        signed_headers(&creator, "post", "/mls/groups"),
        Bytes::from(body.to_string()),
    )
    .await
    .expect("group creation on the epoch-author catalyst must succeed");

    let stored: String =
        sqlx::query_scalar("SELECT epoch_author FROM mls_groups WHERE group_id = $1")
            .bind(&group_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, "peer-a", "the creating catalyst must own the epoch");

    // The creator IS a member, so this reaches the epoch-author guard rather
    // than the membership guard. `commit` is never decoded on this path -- the
    // 409 fires before any base64/MLS parsing -- so a placeholder is fine.
    let commit_path = format!("/mls/groups/{group_id}/commits");
    let commit_body = json!({ "epoch": 1, "commit": "AAAA" });
    let err = submit_commit(
        State(other_catalyst),
        signed_headers(&creator, "post", &commit_path),
        Path(group_id.clone()),
        Bytes::from(commit_body.to_string()),
    )
    .await
    .expect_err("a catalyst that is not the group's epoch author must be refused");

    assert_eq!(
        err.code, 409,
        "wrong-epoch-author must be a 409 (got {}: {})",
        err.code, err.message
    );
    assert_eq!(
        err.error_label.as_deref(),
        Some("wrong epoch author"),
        "the 409 must carry the epoch-author error label"
    );
    assert!(
        err.message.contains("peer-a"),
        "the 409 must name the group's epoch-author catalyst; got: {}",
        err.message
    );

    scratch.drop().await;
}
