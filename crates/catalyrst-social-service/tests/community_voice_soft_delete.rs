//! A soft-deleted community grants no voice-chat authority.
//!
//! Every community-voice entry point authorizes off one lookup -- `Db::community_role` --
//! and none of them loads the community. Because deletion is a soft delete
//! (`rest::handlers::client::communities::delete` and `rest::fed::apply::apply_delete`
//! both only `UPDATE communities SET active = FALSE`, leaving `community_members`
//! untouched), an ex-owner used to keep start/end/kick/mute authority over a room whose
//! community no longer existed.
//!
//! The REST-side twin of this file is `tests/community_soft_delete_standing.rs`.

use std::sync::Arc;

use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_drpc::service_module_definition::ProcedureContext;
use catalyrst_social_service::rpc::config::Config;
use catalyrst_social_service::rpc::db::Db;
use catalyrst_social_service::rpc::proto::v2::{
    end_community_voice_chat_response, start_community_voice_chat_response,
    EndCommunityVoiceChatPayload, SocialServiceServer, StartCommunityVoiceChatPayload,
};
use catalyrst_social_service::rpc::service::{SocialError, SocialServiceImpl};
use catalyrst_social_service::rpc::Context;
use sqlx::PgPool;
use uuid::Uuid;

const FORMER_OWNER: &str = "0x000000000000000000000000000000000dde1e7e";

// A scratch database, not a scratch schema: `sqlx::migrate!` takes a per-database advisory
// lock, so concurrent tests sharing one database deadlock against each other.
async fn setup() -> Option<ScratchDb> {
    let scratch =
        ScratchDb::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_voicedel").await?;
    sqlx::migrate!("./migrations")
        .run(&scratch.pool)
        .await
        .expect("migration run");
    Some(scratch)
}

async fn seed(pool: &PgPool, active: bool, address: &str, role: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted) \
         VALUES ($1, $2, $3, $4, FALSE, $5, FALSE)",
    )
    .bind(id)
    .bind("Voice Soft Delete Community")
    .bind("description")
    .bind(address)
    .bind(active)
    .execute(pool)
    .await
    .expect("seed community");
    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(address)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed member");
    id
}

fn test_cfg() -> Config {
    Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        auth_window_secs: 300,
        database_url: String::new(),
        comms_gatekeeper_url: "http://127.0.0.1:1".into(),
        content_database_url: None,
        content_server_address: String::new(),
        private_voice_chat_expiration_ms: 60_000,
        private_voice_chat_job_interval_ms: 1000,
        private_voice_chat_expiration_batch_size: 20,
        ws_max_concurrent_connections: None,
        ws_max_payload_bytes: 1024 * 1024,
    }
}

fn service_ctx(db: Db, me: &str) -> ProcedureContext<Context> {
    let profiles = catalyrst_social_service::rpc::profiles::Profiles::new(None, String::new());
    let ctx = Context::new(test_cfg(), db, profiles);
    ctx.register_identity(1, me.to_string());
    ProcedureContext {
        server_context: Arc::new(ctx),
        transport_id: 1,
    }
}

/// The lookup every voice entry point shares.
#[tokio::test]
async fn community_role_ignores_members_of_a_soft_deleted_community() {
    let Some(scratch) = setup().await else {
        return;
    };
    let db = Db::new(scratch.pool.clone());

    let live = seed(&scratch.pool, true, FORMER_OWNER, "owner").await;
    assert_eq!(
        db.community_role(&live.to_string(), FORMER_OWNER)
            .await
            .expect("live role lookup"),
        Some("owner".to_string()),
        "an active community must still report its owner"
    );

    let deleted = seed(&scratch.pool, false, FORMER_OWNER, "owner").await;
    assert_eq!(
        db.community_role(&deleted.to_string(), FORMER_OWNER)
            .await
            .expect("deleted role lookup"),
        None,
        "a soft-deleted community must report no role for its former owner"
    );

    let rows_still_present: i64 =
        sqlx::query_scalar("SELECT count(*) FROM community_members WHERE community_id = $1")
            .bind(deleted)
            .fetch_one(&scratch.pool)
            .await
            .expect("count member rows");
    assert_eq!(
        rows_still_present, 1,
        "the membership row must survive the soft delete -- otherwise this test would pass \
         for the wrong reason"
    );

    scratch.drop().await;
}

/// The exploit, at the RPC boundary: `start` is the entry point that opens the room.
#[tokio::test]
async fn a_former_owner_cannot_start_voice_chat_in_a_deleted_community() {
    let Some(scratch) = setup().await else {
        return;
    };
    let db = Db::new(scratch.pool.clone());
    let deleted = seed(&scratch.pool, false, FORMER_OWNER, "owner").await;

    let response =
        <SocialServiceImpl as SocialServiceServer<Context, SocialError>>::start_community_voice_chat(
            &SocialServiceImpl,
            StartCommunityVoiceChatPayload {
                community_id: deleted.to_string(),
            },
            service_ctx(db, FORMER_OWNER),
        )
        .await
        .expect("rpc call");

    assert!(
        matches!(
            response.response,
            Some(start_community_voice_chat_response::Response::ForbiddenError(_))
        ),
        "a former owner of a deleted community must not open a voice room: {response:?}"
    );

    scratch.drop().await;
}

/// The `require_moderator` half of the surface, which the other six entry points share.
#[tokio::test]
async fn a_former_owner_cannot_end_voice_chat_in_a_deleted_community() {
    let Some(scratch) = setup().await else {
        return;
    };
    let db = Db::new(scratch.pool.clone());
    let deleted = seed(&scratch.pool, false, FORMER_OWNER, "owner").await;

    let response =
        <SocialServiceImpl as SocialServiceServer<Context, SocialError>>::end_community_voice_chat(
            &SocialServiceImpl,
            EndCommunityVoiceChatPayload {
                community_id: deleted.to_string(),
            },
            service_ctx(db, FORMER_OWNER),
        )
        .await
        .expect("rpc call");

    assert!(
        matches!(
            response.response,
            Some(end_community_voice_chat_response::Response::ForbiddenError(
                _
            ))
        ),
        "require_moderator must refuse a former owner of a deleted community: {response:?}"
    );

    scratch.drop().await;
}
