use catalyrst_contract_gate::pg::ScratchSchema;
use catalyrst_social_service::rpc::db::Db;
use uuid::Uuid;

async fn connect() -> Option<(Db, ScratchSchema)> {
    let scratch =
        ScratchSchema::create("CATALYRST_SOCIAL_SERVICE_TEST_PG", "cg_social_friends").await?;
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
    let Some((db, scratch)) = connect().await else {
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
    scratch.drop().await;
}

#[tokio::test]
async fn block_with_no_friendship_action_surfaces_via_blocks_table() {
    let Some((db, scratch)) = connect().await else {
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
    scratch.drop().await;
}

#[tokio::test]
async fn get_blocked_users_pages_and_counts_the_full_set() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d5a0";
    let targets = [
        "0x00000000000000000000000000000000f1e9d5b1",
        "0x00000000000000000000000000000000f1e9d5b2",
        "0x00000000000000000000000000000000f1e9d5b3",
    ];
    for t in targets {
        cleanup(&db, A, t).await;
    }
    for t in targets {
        db.block_user(A, t).await.expect("block");
    }

    let page = db.get_blocked_users(A, 2, 0).await.expect("page");
    assert_eq!(page.len(), 2, "limit bounds the page size");

    let total = db.count_blocked_users(A).await.expect("count");
    assert_eq!(
        total, 3,
        "count is the full blocklist size, not the page length"
    );

    let rest = db.get_blocked_users(A, 2, 2).await.expect("page2");
    assert_eq!(rest.len(), 1, "offset walks past the first page");

    for t in targets {
        cleanup(&db, A, t).await;
    }
    scratch.drop().await;
}

#[tokio::test]
async fn friendship_action_outranks_a_block_row() {
    let Some((db, scratch)) = connect().await else {
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
    scratch.drop().await;
}
