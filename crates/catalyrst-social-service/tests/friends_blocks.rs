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

    let (page, total) = db.get_blocked_users(A, 2, 0).await.expect("page");
    assert_eq!(page.len(), 2, "limit bounds the page size");
    assert_eq!(
        total, 3,
        "count is the full blocklist size, not the page length"
    );

    let (rest, total) = db.get_blocked_users(A, 2, 2).await.expect("page2");
    assert_eq!(rest.len(), 1, "offset walks past the first page");
    assert_eq!(total, 3, "the window count rides along on every page");

    let (empty, total) = db.get_blocked_users(A, 2, 9).await.expect("page3");
    assert!(empty.is_empty(), "an offset past the end yields no rows");
    assert_eq!(
        total, 3,
        "an empty page past the end still reports the full count"
    );

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

#[tokio::test]
async fn friendship_probe_reports_last_action_and_both_block_directions() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d506";
    const B: &str = "0x00000000000000000000000000000000f1e9d507";
    cleanup(&db, A, B).await;

    let probe = db.friendship_probe(A, B).await.expect("probe");
    assert!(probe.last.is_none() && !probe.blocked && !probe.blocked_by);

    db.block_user(B, A).await.expect("block");
    let probe = db.friendship_probe(A, B).await.expect("probe");
    assert!(
        probe.last.is_none(),
        "a block alone is not a friendship action"
    );
    assert!(!probe.blocked && probe.blocked_by, "B blocked A");
    let probe = db.friendship_probe(B, A).await.expect("probe");
    assert!(probe.blocked && !probe.blocked_by, "seen from B's side");
    db.unblock_user(B, A).await.expect("unblock");

    let (id, _) = db
        .apply_friendship_action(A, B, "request", false, None, Some("hi"))
        .await
        .expect("request");
    let probe = db.friendship_probe(B, A).await.expect("probe");
    let last = probe.last.expect("the request is the last action");
    assert_eq!(
        (
            last.friendship_id,
            last.action.as_str(),
            last.acting_user.as_str(),
            last.is_active
        ),
        (id, "request", A, false)
    );
    assert_eq!(
        db.last_friendship_action(B, A)
            .await
            .expect("query")
            .map(|l| (l.friendship_id, l.action)),
        Some((id, "request".to_string())),
        "the probe and the plain lookup agree"
    );

    cleanup(&db, A, B).await;
    scratch.drop().await;
}

#[tokio::test]
async fn apply_without_a_known_row_reuses_the_reversed_pair() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d508";
    const B: &str = "0x00000000000000000000000000000000f1e9d509";
    cleanup(&db, A, B).await;

    let (first, _) = db
        .apply_friendship_action(A, B, "request", false, None, None)
        .await
        .expect("request");
    let (second, _) = db
        .apply_friendship_action(B, A, "accept", true, None, None)
        .await
        .expect("a racing insert on the reversed pair resolves to the same row");
    assert_eq!(first, second, "one friendship row per unordered pair");

    let last = db
        .last_friendship_action(A, B)
        .await
        .expect("query")
        .expect("row");
    assert_eq!((last.action.as_str(), last.is_active), ("accept", true));
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM friendships \
         WHERE (address_requester = $1 AND address_requested = $2) \
            OR (address_requester = $2 AND address_requested = $1)",
    )
    .bind(A)
    .bind(B)
    .fetch_one(db.pool())
    .await
    .expect("count");
    assert_eq!(rows, 1);

    let (friends, total) = db.get_friends(A, 10, 0).await.expect("friends");
    assert_eq!((friends, total), (vec![B.to_string()], 1));
    let (friends, total) = db
        .get_friends(A, 10, 5)
        .await
        .expect("friends past the end");
    assert_eq!(
        (friends.len(), total),
        (0, 1),
        "an empty page still carries the count"
    );
    let (requests, total) = db
        .get_friendship_requests(B, true, 10, 0)
        .await
        .expect("requests");
    assert_eq!(
        (requests.len(), total),
        (0, 0),
        "an accepted request is no longer pending"
    );

    cleanup(&db, A, B).await;
    scratch.drop().await;
}

#[tokio::test]
async fn block_and_unblock_return_the_latest_action_in_one_statement() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    const A: &str = "0x00000000000000000000000000000000f1e9d50a";
    const B: &str = "0x00000000000000000000000000000000f1e9d50b";
    cleanup(&db, A, B).await;

    assert!(
        db.block_user_and_last(A, B).await.expect("block").is_none(),
        "no friendship row yet"
    );
    assert!(
        db.is_blocked(A, B).await.expect("query"),
        "the block landed"
    );
    db.block_user_and_last(A, B)
        .await
        .expect("blocking twice is idempotent");

    let (id, _) = db
        .apply_friendship_action(A, B, "request", false, None, None)
        .await
        .expect("request");
    let last = db
        .unblock_user_and_last(A, B)
        .await
        .expect("unblock")
        .expect("row");
    assert_eq!((last.friendship_id, last.action.as_str()), (id, "request"));
    assert!(
        !db.is_blocked(A, B).await.expect("query"),
        "the block is gone"
    );

    let (blocked, blocked_by) = db.get_blocking_status(A).await.expect("status");
    assert!(blocked.is_empty() && blocked_by.is_empty());
    db.block_user(B, A).await.expect("block");
    db.block_user(A, B).await.expect("block");
    let (blocked, blocked_by) = db.get_blocking_status(A).await.expect("status");
    assert_eq!(
        (blocked, blocked_by),
        (vec![B.to_string()], vec![B.to_string()])
    );

    cleanup(&db, A, B).await;
    scratch.drop().await;
}

#[tokio::test]
async fn private_messages_settings_keeps_input_order_and_defaults() {
    let Some((db, scratch)) = connect().await else {
        return;
    };
    const ME: &str = "0x00000000000000000000000000000000f1e9d50c";
    const FRIEND: &str = "0x00000000000000000000000000000000f1e9d50d";
    const STRANGER: &str = "0x00000000000000000000000000000000f1e9d50e";
    const BLOCKED_FRIEND: &str = "0x00000000000000000000000000000000f1e9d50f";
    for t in [FRIEND, STRANGER, BLOCKED_FRIEND] {
        cleanup(&db, ME, t).await;
    }
    for t in [FRIEND, BLOCKED_FRIEND] {
        db.apply_friendship_action(ME, t, "accept", true, None, None)
            .await
            .expect("friend");
    }
    db.block_user(BLOCKED_FRIEND, ME).await.expect("block");
    db.upsert_social_settings(FRIEND, Some("only_friends"), None, None)
        .await
        .expect("settings");

    let rows = db
        .private_messages_settings(
            ME,
            &[
                STRANGER.to_string(),
                FRIEND.to_string(),
                BLOCKED_FRIEND.to_string(),
            ],
        )
        .await
        .expect("settings");
    assert_eq!(
        rows,
        vec![
            (STRANGER.to_string(), "all".to_string(), false),
            (FRIEND.to_string(), "only_friends".to_string(), true),
            (BLOCKED_FRIEND.to_string(), "all".to_string(), false),
        ]
    );
    assert!(db
        .private_messages_settings(ME, &[])
        .await
        .expect("empty")
        .is_empty());

    let unblocked = db.friend_addresses_unblocked(ME).await.expect("friends");
    assert_eq!(
        unblocked,
        vec![FRIEND.to_string()],
        "a block in either direction hides the friend"
    );

    let _ = db.reset_social_settings(FRIEND).await;
    for t in [FRIEND, STRANGER, BLOCKED_FRIEND] {
        cleanup(&db, ME, t).await;
    }
    scratch.drop().await;
}
