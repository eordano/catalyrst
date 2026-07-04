use catalyrst_social_rpc::db::Db;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Duration;

const EXPIRATION_MS: i64 = 60_000;

const EXPIRED_CALLER: &str = "0x00000000000000000000000000000000bee5ca11";
const EXPIRED_CALLEE: &str = "0x000000000000000000000000000000000ca11ee5";
const LIVE_CALLER: &str = "0x0000000000000000000000000000000011ve5a11";
const LIVE_CALLEE: &str = "0x00000000000000000000000000000000011ee5a1";
const SWEEP_CALLER: &str = "0x00000000000000000000000000000000c0ffee01";
const SWEEP_CALLEE: &str = "0x00000000000000000000000000000000c0ffee02";

async fn connect() -> Option<Db> {
    let dsn = std::env::var("CATALYRST_SOCIAL_TEST_DSN").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&dsn)
        .await
        .ok()?;

    sqlx::query(
        "SELECT id, caller_address, callee_address, created_at FROM private_voice_chats LIMIT 0",
    )
    .execute(&pool)
    .await
    .ok()?;
    Some(Db::new(pool))
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
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
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
}

#[tokio::test]
async fn live_call_still_blocks_symmetrically() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
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
}

#[tokio::test]
async fn sweep_reclaims_the_same_rows_the_busy_check_skips() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
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
}
