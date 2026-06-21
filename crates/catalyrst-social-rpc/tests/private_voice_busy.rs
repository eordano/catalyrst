//! Cross-busy guard for private (1:1) voice chats.
//!
//! Proves the fix for audit task #10: the symmetric busy check
//! (`voiceDb.areUsersBeingCalledOrCallingSomeone`) must NOT report a user as
//! busy on the strength of a stale, past-TTL row. Such rows are only swept
//! lazily by the background expiry job, so without a `created_at`-window filter
//! on the busy query they would wedge a user with a false `ConflictingError`
//! until the next sweep.
//!
//! Post-alignment to upstream's FINAL schema (migration
//! `1749835946066_expire-private-voice-chats`) the table no longer has an
//! `expires_at` column: liveness is derived from `created_at` + the configured
//! `PRIVATE_VOICE_CHAT_EXPIRATION_TIME`. A "stale" row is therefore one whose
//! `created_at` is older than that window; this test backdates `created_at`
//! directly to model a row that has out-lived its TTL but not yet been reclaimed
//! by the sweep.
//!
//! This is a DB-backed integration test. It connects to a local `social`
//! cluster (DSN from the `CATALYRST_SOCIAL_TEST_DSN` env var); if that var is
//! unset or the cluster is unreachable (e.g. CI without infra) the test skips
//! rather than failing, mirroring the lazy-connect pattern in `ws_handshake.rs`.

use catalyrst_social_rpc::db::Db;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::time::Duration;

// TTL window the busy check / sweep apply, in ms. Matches the default config
// `private_voice_chat_expiration_ms` (60000); the tests pass it explicitly so
// they don't depend on env.
const EXPIRATION_MS: i64 = 60_000;

// Per-test, distinct, lowercase, well-formed-looking addresses. Each test owns
// its own caller/callee pair so the two tests never collide on the unique
// caller/callee constraints when the suite runs in parallel (no
// `--test-threads=1` needed), and so neither collides with real call rows.
const EXPIRED_CALLER: &str = "0x00000000000000000000000000000000bee5ca11";
const EXPIRED_CALLEE: &str = "0x000000000000000000000000000000000ca11ee5";
const LIVE_CALLER: &str = "0x0000000000000000000000000000000011ve5a11";
const LIVE_CALLEE: &str = "0x00000000000000000000000000000000011ee5a1";
const SWEEP_CALLER: &str = "0x00000000000000000000000000000000c0ffee01";
const SWEEP_CALLEE: &str = "0x00000000000000000000000000000000c0ffee02";

/// Connect to the local `social` cluster (DSN from `CATALYRST_SOCIAL_TEST_DSN`)
/// with a short timeout, or return `None` so the test can skip when it isn't
/// configured or the cluster isn't running.
async fn connect() -> Option<Db> {
    let dsn = std::env::var("CATALYRST_SOCIAL_TEST_DSN").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&dsn)
        .await
        .ok()?;
    // Probe the post-alignment schema we depend on (no expires_at column); bail
    // (skip) if it isn't present.
    sqlx::query(
        "SELECT id, caller_address, callee_address, created_at FROM private_voice_chats LIMIT 0",
    )
    .execute(&pool)
    .await
    .ok()?;
    Some(Db::new(pool))
}

/// Remove any test rows left over from a prior run so the unique constraints
/// don't masquerade as a busy state. Scoped to the given pair so the two tests
/// never clobber each other's rows under the parallel runner.
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

/// Backdate a row's `created_at` by `ms` milliseconds so it lands outside the
/// liveness window (modelling a row that out-lived its TTL but is still present
/// because the sweep hasn't reclaimed it yet).
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

    // A stale call: insert, then push its created_at well past the TTL window so
    // it faithfully models a row that has out-lived its TTL but not yet been
    // reclaimed by the sweep (the schema has no expires_at column post-alignment).
    let stale_id = db
        .start_private_voice_chat(EXPIRED_CALLER, EXPIRED_CALLEE)
        .await
        .expect("insert stale call");
    backdate(&db, stale_id, EXPIRATION_MS + 3_600_000).await;

    // The row physically exists...
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

    // ...but because it is past-TTL, the busy guard must NOT treat either party
    // as busy. Pre-fix (no created_at-window filter) this returned true -> false
    // ConflictingError.
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

    // A live call (fresh created_at, inside the TTL window) must keep blocking,
    // and do so symmetrically: the busy guard fires whether the new request
    // names the caller or callee, and whether they would take the caller or
    // callee column.
    let live_id = db
        .start_private_voice_chat(LIVE_CALLER, LIVE_CALLEE)
        .await
        .expect("insert live call");

    for probe in [
        vec![LIVE_CALLER.to_string()], // existing caller as caller
        vec![LIVE_CALLEE.to_string()], // existing callee as callee
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

/// The sweep deletes exactly the rows the busy check skips: a row backdated past
/// the TTL window is reclaimed, and once gone the busy guard is clear.
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
