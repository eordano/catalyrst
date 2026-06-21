use catalyrst_social_service::rpc::db::Db;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use uuid::Uuid;

async fn connect() -> Option<Db> {
    let url = std::env::var("CATALYRST_SOCIAL_SERVICE_TEST_PG").ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&url)
        .await
        .ok()?;
    let schema = format!("test_social_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&format!("{url}?options=-c%20search_path%3D{schema}"))
        .await
        .ok()?;
    for sql in [
        include_str!("../migrations/0008_social.sql"),
        include_str!("../migrations/0009_friendships_unordered_unique.sql"),
        include_str!("../migrations/0010_expire_private_voice_chats.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.expect("migration");
    }
    Some(Db::new(pool))
}

async fn cleanup(db: &Db, a: &str, b: &str) {
    let _ = sqlx::query(
        "DELETE FROM blocks WHERE blocker_address IN ($1, $2) OR blocked_address IN ($1, $2)",
    )
    .bind(a)
    .bind(b)
    .execute(db.pool())
    .await;

    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM friendships \
         WHERE (address_requester = $1 AND address_requested = $2) \
            OR (address_requester = $2 AND address_requested = $1)",
    )
    .bind(a)
    .bind(b)
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
    const A: &str = "0x00000000000000000000000000000000f1e9d500";
    const B: &str = "0x00000000000000000000000000000000f1e9d501";
    cleanup(&db, A, B).await;

    assert!(
        !db.is_friendship_blocked(A, B).await.expect("query"),
        "no block should report not-blocked"
    );

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

    cleanup(&db, A, B).await;
}

#[tokio::test]
async fn block_with_no_friendship_action_surfaces_via_blocks_table() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d502";
    const B: &str = "0x00000000000000000000000000000000f1e9d503";
    cleanup(&db, A, B).await;

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

    cleanup(&db, A, B).await;
}

#[tokio::test]
async fn friendship_action_outranks_a_block_row() {
    let Some(db) = connect().await else {
        eprintln!("skipping: `social` cluster unavailable");
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d504";
    const B: &str = "0x00000000000000000000000000000000f1e9d505";
    cleanup(&db, A, B).await;

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

    cleanup(&db, A, B).await;
}
