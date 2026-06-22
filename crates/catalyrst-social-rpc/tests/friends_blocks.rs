//! Friends-RPC correctness (capability #14): block enforcement on upsert and
//! the block-vs-friendship precedence of `getFriendshipStatus`.
//!
//! Two ground-truth behaviours from `social-service-ea`:
//!   * `friends.upsertFriendship` (logic/friends/component.ts) calls
//!     `friendsDb.isFriendshipBlocked(a, b)` and rejects (BlockedUserError ->
//!     invalidFriendshipAction) when EITHER party has blocked the other.
//!   * `friendsDb.getLastFriendshipActionByUsers` (adapters/friends-db.ts):
//!     the latest *friendship action* always wins; the blocks table is consulted
//!     ONLY when no friendship action exists, where it yields a synthetic BLOCK
//!     whose acting_user is the blocker.
//!
//! These are DB-backed integration tests against a local `social` cluster (DSN
//! from `CATALYRST_SOCIAL_TEST_DSN`). They skip (rather than fail) when that var
//! is unset or the cluster is unreachable, mirroring `private_voice_busy.rs`.
//! The pure precedence mapping itself is unit-tested in `src/service.rs`; this
//! file proves the DB plumbing those decisions rest on.

use catalyrst_social_rpc::db::Db;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use uuid::Uuid;

// Per-test, distinct, lowercase, well-formed-looking addresses so parallel runs
// never collide on each other's rows or on real data.
const A: &str = "0x00000000000000000000000000000000f1e9d500";
const B: &str = "0x00000000000000000000000000000000f1e9d501";

async fn connect() -> Option<Db> {
    let dsn = std::env::var("CATALYRST_SOCIAL_TEST_DSN").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&dsn)
        .await
        .ok()?;
    // Probe the tables we depend on; skip if the schema isn't present.
    sqlx::query("SELECT 1 FROM friendships LIMIT 0")
        .execute(&pool)
        .await
        .ok()?;
    sqlx::query("SELECT 1 FROM blocks LIMIT 0")
        .execute(&pool)
        .await
        .ok()?;
    Some(Db::new(pool))
}

async fn cleanup(db: &Db) {
    let _ = sqlx::query(
        "DELETE FROM blocks WHERE blocker_address IN ($1, $2) OR blocked_address IN ($1, $2)",
    )
    .bind(A)
    .bind(B)
    .execute(db.pool())
    .await;
    // friendship_actions cascade off friendships in upstream schema; delete both
    // defensively in case the FK isn't ON DELETE CASCADE locally.
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM friendships \
         WHERE (address_requester = $1 AND address_requested = $2) \
            OR (address_requester = $2 AND address_requested = $1)",
    )
    .bind(A)
    .bind(B)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();
    for id in ids {
        let _ = sqlx::query("DELETE FROM friendship_actions WHERE friendship_id = $1")
            .bind(id)
            .execute(db.pool())
            .await;
        let _ = sqlx::query("DELETE FROM friendships WHERE id = $1")
            .bind(id)
            .execute(db.pool())
            .await;
    }
}

#[tokio::test]
async fn is_friendship_blocked_is_bidirectional() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
        return;
    };
    cleanup(&db).await;

    // No block yet.
    assert!(
        !db.is_friendship_blocked(A, B).await.expect("query"),
        "no block should report not-blocked"
    );

    // A blocks B -> upsert must be rejected for BOTH (A->B and B->A) directions.
    db.block_user(A, B).await.expect("block");
    assert!(
        db.is_friendship_blocked(A, B).await.expect("query"),
        "blocker side must be reported as blocked"
    );
    assert!(
        db.is_friendship_blocked(B, A).await.expect("query"),
        "blocked side must ALSO be reported as blocked (the reported defect)"
    );

    db.unblock_user(A, B).await.expect("unblock");
    assert!(
        !db.is_friendship_blocked(A, B).await.expect("query"),
        "after unblock, no longer blocked"
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn block_with_no_friendship_action_surfaces_via_blocks_table() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
        return;
    };
    cleanup(&db).await;

    // A blocks B with no prior friendship: there is no friendship action, so
    // `last_friendship_action` is None and the status must come from the blocks
    // table (Blocked for A, BlockedBy for B). This is the precedence fallback.
    db.block_user(A, B).await.expect("block");

    assert!(
        db.last_friendship_action(A, B)
            .await
            .expect("query")
            .is_none(),
        "blocking without a friendship must not create a friendship action"
    );
    assert!(
        db.is_blocked(A, B).await.expect("query"),
        "A is the blocker"
    );
    assert!(
        !db.is_blocked(B, A).await.expect("query"),
        "B did not block A"
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn friendship_action_outranks_a_block_row() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
        return;
    };
    cleanup(&db).await;

    // Establish an accepted friendship action, then drop a raw block row in the
    // table WITHOUT recording a BLOCK friendship action. Upstream precedence: the
    // friendship action ("accept") wins; the block row is ignored for status.
    let (id, _) = db
        .apply_friendship_action(A, B, "request", false, None, Some("hi"))
        .await
        .expect("request");
    db.apply_friendship_action(B, A, "accept", true, Some(id), None)
        .await
        .expect("accept");
    db.block_user(A, B).await.expect("block");

    let last = db
        .last_friendship_action(A, B)
        .await
        .expect("query")
        .expect("a friendship action exists");
    assert_eq!(
        last.action, "accept",
        "the latest friendship action must win over the raw block row"
    );

    cleanup(&db).await;
}
