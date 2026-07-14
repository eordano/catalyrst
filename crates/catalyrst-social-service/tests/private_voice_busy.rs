use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_drpc::service_module_definition::ProcedureContext;
use catalyrst_social_service::rpc::config::Config;
use catalyrst_social_service::rpc::db::Db;
use catalyrst_social_service::rpc::proto::v2::{
    start_private_voice_chat_response, SocialServiceServer, StartPrivateVoiceChatPayload, User,
};
use catalyrst_social_service::rpc::service::{SocialError, SocialServiceImpl};
use catalyrst_social_service::rpc::Context;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::sync::Arc;

const EXPIRATION_MS: i64 = 60_000;

const EXPIRED_CALLER: &str = "0x00000000000000000000000000000000bee5ca11";
const EXPIRED_CALLEE: &str = "0x000000000000000000000000000000000ca11ee5";
const LIVE_CALLER: &str = "0x0000000000000000000000000000000011ve5a11";
const LIVE_CALLEE: &str = "0x00000000000000000000000000000000011ee5a1";
const SWEEP_CALLER: &str = "0x00000000000000000000000000000000c0ffee01";
const SWEEP_CALLEE: &str = "0x00000000000000000000000000000000c0ffee02";

async fn connect() -> Option<(Db, ScratchSchema)> {
    let scratch =
        ScratchSchema::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_voice").await?;
    for sql in [
        include_str!("../migrations/0008_social.sql"),
        include_str!("../migrations/0009_friendships_unordered_unique.sql"),
        include_str!("../migrations/0010_expire_private_voice_chats.sql"),
    ] {
        sqlx::raw_sql(sql)
            .execute(&scratch.pool)
            .await
            .expect("migration");
    }
    let db = Db::new(scratch.pool.clone());
    Some((db, scratch))
}

async fn cleanup(db: &Db, caller: &str, callee: &str) {
    let _ = sqlx::query(
        "DELETE FROM private_voice_chats \
         WHERE caller_address IN ($1, $2) OR callee_address IN ($1, $2)",
    )
    .bind(caller)
    .bind(callee)
    .execute(db.pool())
    .await;
}

async fn backdate(db: &Db, id: sqlx::types::Uuid, ms: i64) {
    sqlx::query(
        "UPDATE private_voice_chats \
         SET created_at = now()::timestamp - ($2 * interval '1 millisecond') WHERE id = $1",
    )
    .bind(id)
    .bind(ms)
    .execute(db.pool())
    .await
    .expect("backdate created_at");
}

#[tokio::test]
async fn expired_prior_call_does_not_block_new_one() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    cleanup(&db, EXPIRED_CALLER, EXPIRED_CALLEE).await;

    let stale_id = db
        .start_private_voice_chat(EXPIRED_CALLER, EXPIRED_CALLEE)
        .await
        .expect("insert stale call");
    backdate(&db, stale_id, EXPIRATION_MS + 3_600_000).await;

    let still_there: bool =
        sqlx::query("SELECT EXISTS(SELECT 1 FROM private_voice_chats WHERE id = $1) AS e")
            .bind(stale_id)
            .fetch_one(db.pool())
            .await
            .expect("probe stale row")
            .get::<bool, _>("e");
    assert!(
        still_there,
        "stale row should still be present (not yet swept)"
    );

    let busy = db
        .are_users_being_called_or_calling_someone(
            &[EXPIRED_CALLER.to_string(), EXPIRED_CALLEE.to_string()],
            EXPIRATION_MS,
        )
        .await
        .expect("busy check");
    assert!(
        !busy,
        "an expired prior call must not block a new one (false ConflictingError)"
    );

    cleanup(&db, EXPIRED_CALLER, EXPIRED_CALLEE).await;
    scratch.drop().await;
}

#[tokio::test]
async fn live_call_still_blocks_symmetrically() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    cleanup(&db, LIVE_CALLER, LIVE_CALLEE).await;

    let live_id = db
        .start_private_voice_chat(LIVE_CALLER, LIVE_CALLEE)
        .await
        .expect("insert live call");

    for probe in [
        vec![LIVE_CALLER.to_string()],
        vec![LIVE_CALLEE.to_string()],
        vec![
            "0xdead0000000000000000000000000000000beef0".to_string(),
            LIVE_CALLER.to_string(),
        ],
        vec![
            LIVE_CALLEE.to_string(),
            "0xfeed0000000000000000000000000000000beef0".to_string(),
        ],
    ] {
        let busy = db
            .are_users_being_called_or_calling_someone(&probe, EXPIRATION_MS)
            .await
            .expect("busy check");
        assert!(busy, "a live call must block (probe={probe:?})");
    }

    db.delete_private_voice_chat(live_id)
        .await
        .expect("delete live call");
    cleanup(&db, LIVE_CALLER, LIVE_CALLEE).await;
    scratch.drop().await;
}

#[tokio::test]
async fn sweep_reclaims_the_same_rows_the_busy_check_skips() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    cleanup(&db, SWEEP_CALLER, SWEEP_CALLEE).await;

    let stale_id = db
        .start_private_voice_chat(SWEEP_CALLER, SWEEP_CALLEE)
        .await
        .expect("insert stale call");
    backdate(&db, stale_id, EXPIRATION_MS + 3_600_000).await;

    let reclaimed = db
        .expire_private_voice_chats(EXPIRATION_MS, 20)
        .await
        .expect("sweep");
    assert!(
        reclaimed.iter().any(|(id, _, _)| *id == stale_id),
        "the sweep must reclaim the stale row the busy check skipped"
    );

    let still_there: bool =
        sqlx::query("SELECT EXISTS(SELECT 1 FROM private_voice_chats WHERE id = $1) AS e")
            .bind(stale_id)
            .fetch_one(db.pool())
            .await
            .expect("probe swept row")
            .get::<bool, _>("e");
    assert!(!still_there, "swept stale row must be gone");

    cleanup(&db, SWEEP_CALLER, SWEEP_CALLEE).await;
    scratch.drop().await;
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
        private_voice_chat_expiration_ms: EXPIRATION_MS,
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

async fn start_call(
    db: Db,
    me: &str,
    callee: &str,
) -> Result<catalyrst_social_service::rpc::proto::v2::StartPrivateVoiceChatResponse, SocialError> {
    <SocialServiceImpl as SocialServiceServer<Context, SocialError>>::start_private_voice_chat(
        &SocialServiceImpl,
        StartPrivateVoiceChatPayload {
            callee: Some(User {
                address: callee.to_string(),
            }),
        },
        service_ctx(db, me),
    )
    .await
}

#[tokio::test]
async fn self_call_is_forbidden() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/unused")
        .expect("lazy pool");
    let response = start_call(Db::new(pool), SWEEP_CALLER, SWEEP_CALLER)
        .await
        .expect("rpc call");
    assert!(
        matches!(
            response.response,
            Some(start_private_voice_chat_response::Response::ForbiddenError(
                _
            ))
        ),
        "a self-call must be forbidden: {response:?}"
    );
}

#[tokio::test]
async fn blocked_pair_cannot_start_call() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    if sqlx::query("SELECT 1 FROM blocks LIMIT 0")
        .execute(db.pool())
        .await
        .is_err()
    {
        return;
    }

    let caller = "0x00000000000000000000000000000000b10cca11";
    let callee = "0x00000000000000000000000000000000b10ccee5";
    let unblock = |db: &Db| {
        let db = db.clone();
        async move {
            let _ = sqlx::query(
                "DELETE FROM blocks WHERE blocker_address IN ($1, $2) OR blocked_address IN ($1, $2)",
            )
            .bind(caller)
            .bind(callee)
            .execute(db.pool())
            .await;
        }
    };
    unblock(&db).await;

    db.block_user(callee, caller).await.expect("block");
    let response = start_call(db.clone(), caller, callee)
        .await
        .expect("rpc call");
    assert!(
        matches!(
            response.response,
            Some(start_private_voice_chat_response::Response::ForbiddenError(
                _
            ))
        ),
        "a blocked pair must not start a call: {response:?}"
    );

    unblock(&db).await;
    scratch.drop().await;
}
