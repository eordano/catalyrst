// 1. The ACL model (HIGH): `pick_in_lists` reaches any list the caller may
//    edit -- their own, the shared default Wishlist, or one carrying an edit
//    grant for them -- and nothing else (upstream 863b04c). The compensating
//    upstream invariant still holds: every picks read in `get_lists` is
//    scoped to the caller. A pick legitimately placed by a grantee must never
//    move the owner's itemsCount / preview / isItemInList.
// 1b. Authorization (upstream lists-authorization.spec.ts): another user's
//    private list with zero ACL rows is flagged for a non-owner and receives
//    no pick even when the write is attempted directly; the default list, the
//    caller's own list and an edit-granted list are all editable.
// 2. The shared default Wishlist (migration 0010) is surfaced by
//    `get_lists` for every caller, flagged `is_default_list`, sorted first,
//    and included in the total.
// 3. `get_picks_by_list_id` dedups on the full pick identity (upstream's
//    row-level DISTINCT): two users' picks of the same item in a shared list
//    are two rows even with identical (ms-truncated) created_at. This also
//    exercises 0010's PK widening to (item_id, user_address, list_id).
// 4. The preview is the caller's 4 OLDEST picks, ascending (upstream
//    `(ARRAY_REMOVE(ARRAY_AGG(p.item_id ORDER BY p.created_at), NULL))[:4]`),
//    not the 4 newest descending.
// 5. `is_private` is ACL-derived per caller (component.ts:114), never the
//    stored column -- the seeded shared Wishlist (stored false, no ACL rows)
//    reads private, a grant to another wallet stays private for this caller,
//    and only a caller/'*' grant flips it public.
// Set CATALYRST_MARKET_TEST_PG to run; each test builds a throwaway database
// and drops it on the way out.

use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_market::ports::lists::{
    GetListsOptions, ListSortBy, ListSortDirection, ListsComponent, DEFAULT_LIST_ID,
    DEFAULT_LIST_USER_ADDRESS,
};
use sqlx::{PgPool, Row};

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const WALLET_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WALLET_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ITEM_X: &str = "0x1111111111111111111111111111111111111111-0";
const ITEM_Y: &str = "0x2222222222222222222222222222222222222222-0";

async fn build_scratch() -> Option<ScratchDb> {
    let scratch = ScratchDb::builder(PG_VAR, "cg_mkt_fav")
        .schemas(["favorites"])
        .build()
        .await?;
    // The real migration files (raw_sql handles 0010's DO $$ guard block).
    sqlx::raw_sql(include_str!("../migrations/0006_favorites_lists.sql"))
        .execute(&scratch.pool)
        .await
        .expect("0006 applies");
    sqlx::raw_sql(include_str!(
        "../migrations/0010_favorites_shared_default_list.sql"
    ))
    .execute(&scratch.pool)
    .await
    .expect("0010 applies");
    Some(scratch)
}

fn opts<'a>(item_id: Option<&'a str>) -> GetListsOptions<'a> {
    GetListsOptions {
        limit: 100,
        offset: 0,
        sort_by: ListSortBy::CreatedAt,
        sort_direction: ListSortDirection::Desc,
        item_id,
        q: None,
    }
}

async fn insert_list(pool: &PgPool, name: &str, owner: &str) -> String {
    sqlx::query(
        "INSERT INTO favorites.lists (name, user_address, is_private) \
         VALUES ($1, $2, true) RETURNING id::text AS id",
    )
    .bind(name)
    .bind(owner)
    .fetch_one(pool)
    .await
    .unwrap()
    .try_get::<String, _>("id")
    .unwrap()
}

async fn grant_edit(pool: &PgPool, list_id: &str, grantee: &str) {
    sqlx::query(
        "INSERT INTO favorites.acl (list_id, permission, grantee) VALUES ($1::uuid, 'edit', $2)",
    )
    .bind(list_id)
    .bind(grantee)
    .execute(pool)
    .await
    .unwrap();
}

async fn picks_in(pool: &PgPool, list_id: &str) -> i64 {
    sqlx::query("SELECT COUNT(*)::int8 AS n FROM favorites.picks WHERE list_id = $1::uuid")
        .bind(list_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .try_get("n")
        .unwrap()
}

/// Upstream lists-authorization.spec.ts (863b04c): the victim's private list
/// carries zero ACL rows -- the normal state of a private list -- and the
/// attacker's bulk pick must be rejected. The pre-fix LEFT JOIN plus negative
/// comparison evaluated UNKNOWN there and let the write through.
#[tokio::test]
async fn a_private_list_without_acl_rows_rejects_a_non_owner() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());
    let victim_list = insert_list(&scratch.pool, "victim private list", WALLET_A).await;

    let flagged = lists
        .check_non_editable_lists(std::slice::from_ref(&victim_list), WALLET_B)
        .await
        .unwrap();
    assert_eq!(flagged, vec![victim_list.clone()]);

    // Defence in depth: even a caller that skips the guard writes nothing.
    lists
        .pick_in_lists(ITEM_Y, WALLET_B, std::slice::from_ref(&victim_list))
        .await
        .unwrap();
    assert_eq!(picks_in(&scratch.pool, &victim_list).await, 0);

    scratch.drop().await;
}

#[tokio::test]
async fn the_default_list_own_lists_and_edit_granted_lists_are_editable() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());
    let own_list = insert_list(&scratch.pool, "attacker own list", WALLET_B).await;
    let shared_list = insert_list(&scratch.pool, "victim shared list", WALLET_A).await;
    grant_edit(&scratch.pool, &shared_list, WALLET_B).await;
    let public_list = insert_list(&scratch.pool, "everyone edits", WALLET_A).await;
    grant_edit(&scratch.pool, &public_list, "*").await;

    let targets = [
        DEFAULT_LIST_ID.to_string(),
        own_list.clone(),
        shared_list.clone(),
        public_list.clone(),
    ];
    let flagged = lists
        .check_non_editable_lists(&targets, WALLET_B)
        .await
        .unwrap();
    assert!(flagged.is_empty(), "{flagged:?}");
    lists
        .pick_in_lists(ITEM_Y, WALLET_B, &targets)
        .await
        .unwrap();
    for list in &targets {
        assert_eq!(picks_in(&scratch.pool, list).await, 1, "{list}");
    }

    // A view grant is not an edit grant.
    let viewable = insert_list(&scratch.pool, "view only", WALLET_A).await;
    sqlx::query(
        "INSERT INTO favorites.acl (list_id, permission, grantee) VALUES ($1::uuid, 'view', $2)",
    )
    .bind(&viewable)
    .bind(WALLET_B)
    .execute(&scratch.pool)
    .await
    .unwrap();
    let flagged = lists
        .check_non_editable_lists(std::slice::from_ref(&viewable), WALLET_B)
        .await
        .unwrap();
    assert_eq!(flagged, vec![viewable]);

    scratch.drop().await;
}

/// A grantee's pick lands in the owner's list, and the owner's GET /v1/lists
/// still comes back with the owner's OWN counts: itemsCount 1, preview [X],
/// isItemInList(Y) false.
#[tokio::test]
async fn foreign_pick_does_not_move_the_owners_counts_or_preview() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    let list_id = insert_list(&scratch.pool, "summer fits", WALLET_A).await;
    lists
        .pick_in_lists(ITEM_X, WALLET_A, std::slice::from_ref(&list_id))
        .await
        .unwrap();

    grant_edit(&scratch.pool, &list_id, WALLET_B).await;
    let non_editable = lists
        .check_non_editable_lists(std::slice::from_ref(&list_id), WALLET_B)
        .await
        .unwrap();
    assert!(
        non_editable.is_empty(),
        "an edit grant makes the list editable"
    );
    lists
        .pick_in_lists(ITEM_Y, WALLET_B, std::slice::from_ref(&list_id))
        .await
        .unwrap();
    let foreign_picks: i64 =
        sqlx::query("SELECT COUNT(*)::int8 AS n FROM favorites.picks WHERE list_id = $1::uuid")
            .bind(&list_id)
            .fetch_one(&scratch.pool)
            .await
            .unwrap()
            .try_get("n")
            .unwrap();
    assert_eq!(foreign_picks, 2, "both picks physically exist in the list");

    // A's read is scoped to A: the foreign pick is invisible.
    let (rows, _) = lists
        .get_lists(WALLET_A, &opts(Some(ITEM_Y)))
        .await
        .unwrap();
    let l = rows
        .iter()
        .find(|r| r.id == list_id)
        .expect("owner sees own list");
    assert_eq!(
        l.items_count, 1,
        "foreign pick inflated the owner's itemsCount"
    );
    assert_eq!(
        l.preview_of_item_ids,
        vec![ITEM_X.to_string()],
        "foreign pick leaked into the owner's preview"
    );
    assert_eq!(
        l.is_item_in_list,
        Some(false),
        "isItemInList must reflect the CALLER's picks only"
    );

    let (rows, _) = lists
        .get_lists(WALLET_A, &opts(Some(ITEM_X)))
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .find(|r| r.id == list_id)
            .unwrap()
            .is_item_in_list,
        Some(true)
    );

    // B never sees A's list at all (only B's own lists + the shared default).
    let (rows, _) = lists.get_lists(WALLET_B, &opts(None)).await.unwrap();
    assert!(rows.iter().all(|r| r.id != list_id));

    scratch.drop().await;
}

/// Migration 0010's shared default Wishlist is actually surfaced: visible to
/// every caller, `is_default_list = true`, sorted ahead of the caller's own
/// lists, counted in the total -- and its per-caller itemsCount stays scoped
/// (it must not become a global counter across all users).
#[tokio::test]
async fn shared_default_wishlist_is_surfaced_first_and_caller_scoped() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    let own_list = insert_list(&scratch.pool, "own", WALLET_A).await;

    for (item, wallet) in [(ITEM_X, WALLET_A), (ITEM_X, WALLET_B), (ITEM_Y, WALLET_B)] {
        let flagged = lists
            .check_non_editable_lists(&[DEFAULT_LIST_ID.to_string()], wallet)
            .await
            .unwrap();
        assert!(flagged.is_empty(), "everyone may edit the shared Wishlist");
        lists
            .pick_in_lists(item, wallet, &[DEFAULT_LIST_ID.to_string()])
            .await
            .unwrap();
    }

    let (rows, total) = lists.get_lists(WALLET_A, &opts(None)).await.unwrap();
    assert_eq!(total, 2, "count query must include the shared Wishlist");
    assert_eq!(rows.len(), 2);
    let wishlist = &rows[0];
    assert_eq!(
        wishlist.id, DEFAULT_LIST_ID,
        "default list sorts ahead of the caller's own lists"
    );
    assert!(wishlist.is_default_list);
    assert_eq!(wishlist.user_address, DEFAULT_LIST_USER_ADDRESS);
    assert_eq!(
        wishlist.items_count, 1,
        "the shared Wishlist's itemsCount is the CALLER's picks, not a global counter"
    );
    assert_eq!(wishlist.preview_of_item_ids, vec![ITEM_X.to_string()]);
    assert!(!rows[1].is_default_list);
    assert_eq!(rows[1].id, own_list);

    let (rows, _) = lists.get_lists(WALLET_B, &opts(None)).await.unwrap();
    assert_eq!(rows[0].id, DEFAULT_LIST_ID);
    assert_eq!(rows[0].items_count, 2, "B's own two picks");

    scratch.drop().await;
}

/// Pin 4: with five picks at strictly increasing created_at, the preview is
/// the FIRST four in pick order (oldest, ascending) -- upstream aggregates
/// `ORDER BY p.created_at` and slices the head with `[:4]`. The pre-parity
/// port returned the 4 newest descending, which this pin must catch.
#[tokio::test]
async fn preview_is_the_callers_four_oldest_picks_ascending() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    let list_id = insert_list(&scratch.pool, "ordered", WALLET_A).await;
    let items: Vec<String> = (1..=5u32)
        .map(|i| format!("0x{:040x}-0", 0xf000 + i))
        .collect();
    for (i, item) in items.iter().enumerate() {
        lists
            .pick_in_lists(item, WALLET_A, std::slice::from_ref(&list_id))
            .await
            .unwrap();
        // Deterministic strictly-increasing pick times, oldest first.
        sqlx::query(
            "UPDATE favorites.picks \
             SET created_at = TIMESTAMPTZ '2026-01-01T00:00:00Z' + ($1 * INTERVAL '1 minute') \
             WHERE list_id = $2::uuid AND item_id = $3",
        )
        .bind(i as i32)
        .bind(&list_id)
        .bind(item)
        .execute(&scratch.pool)
        .await
        .unwrap();
    }

    let (rows, _) = lists.get_lists(WALLET_A, &opts(None)).await.unwrap();
    let l = rows.iter().find(|r| r.id == list_id).unwrap();
    assert_eq!(l.items_count, 5, "the count still covers all five picks");
    assert_eq!(
        l.preview_of_item_ids,
        items[..4].to_vec(),
        "preview must be the 4 OLDEST picks in ascending pick order"
    );

    scratch.drop().await;
}

/// Pin 5: `is_private` derives from the ACL per caller, not the stored
/// column. The shared Wishlist (stored `is_private = false`, zero ACL rows)
/// reads private; so does an owned list whose only grant names another
/// wallet; a `'*'` (or caller) grant flips it public.
#[tokio::test]
async fn is_private_is_derived_from_the_acl_not_the_stored_column() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    // insert_list stores is_private = true; migration 0010 seeds the shared
    // Wishlist with stored is_private = false. Neither value may leak out.
    let own = insert_list(&scratch.pool, "own", WALLET_A).await;

    let find = |rows: &[catalyrst_market::ports::lists::FavoriteList], id: &str| {
        rows.iter()
            .find(|r| r.id == id)
            .map(|r| r.is_private)
            .expect("list visible")
    };

    let (rows, _) = lists.get_lists(WALLET_A, &opts(None)).await.unwrap();
    assert!(
        find(&rows, DEFAULT_LIST_ID),
        "the shared Wishlist has no ACL rows: private, though the stored column says false"
    );
    assert!(find(&rows, &own), "no ACL rows means private");

    // A grant to somebody ELSE leaves the list private for this caller...
    sqlx::query(
        "INSERT INTO favorites.acl (list_id, permission, grantee) VALUES ($1::uuid, 'view', $2)",
    )
    .bind(&own)
    .bind(WALLET_B)
    .execute(&scratch.pool)
    .await
    .unwrap();
    let (rows, _) = lists.get_lists(WALLET_A, &opts(None)).await.unwrap();
    assert!(
        find(&rows, &own),
        "a grant to another wallet must not read public for this caller"
    );

    // ...and the '*' wildcard grant flips it public.
    sqlx::query(
        "INSERT INTO favorites.acl (list_id, permission, grantee) VALUES ($1::uuid, 'view', '*')",
    )
    .bind(&own)
    .execute(&scratch.pool)
    .await
    .unwrap();
    let (rows, _) = lists.get_lists(WALLET_A, &opts(None)).await.unwrap();
    assert!(!find(&rows, &own), "a '*' grant reads public");

    scratch.drop().await;
}

/// Upstream's `SELECT DISTINCT(p.item_id), p.*` is row-level: the same item
/// picked by two users in a shared list yields two rows. Ours must not
/// collapse them when the ms-truncated created_at collides (the picks_count
/// window counts both either way). Also exercises 0010's PK widening -- under
/// the old (item_id, list_id) key B's pick would have been silently dropped.
#[tokio::test]
async fn same_item_picked_by_two_users_stays_two_rows() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    for wallet in [WALLET_A, WALLET_B] {
        lists
            .pick_in_lists(ITEM_X, wallet, &[DEFAULT_LIST_ID.to_string()])
            .await
            .unwrap();
    }
    // Force the ms-truncated created_at to collide.
    sqlx::query(
        "UPDATE favorites.picks SET created_at = '2026-01-01T00:00:00Z' \
         WHERE list_id = $1::uuid",
    )
    .bind(DEFAULT_LIST_ID)
    .execute(&scratch.pool)
    .await
    .unwrap();
    // Make the list ACL-public so one caller can see both users' picks.
    sqlx::query(
        "INSERT INTO favorites.acl (list_id, permission, grantee) VALUES ($1::uuid, 'view', '*')",
    )
    .bind(DEFAULT_LIST_ID)
    .execute(&scratch.pool)
    .await
    .unwrap();

    let (picks, count) = lists
        .get_picks_by_list_id(DEFAULT_LIST_ID, None, 100, 0)
        .await
        .unwrap();
    assert_eq!(
        picks.len(),
        2,
        "two users' picks of the same item are two rows (row-level DISTINCT)"
    );
    assert!(picks.iter().all(|p| p.item_id == ITEM_X));
    assert_eq!(count, 2);

    // A signed caller without ACL visibility still sees only their own pick.
    sqlx::query("DELETE FROM favorites.acl WHERE list_id = $1::uuid")
        .bind(DEFAULT_LIST_ID)
        .execute(&scratch.pool)
        .await
        .unwrap();
    let (picks, count) = lists
        .get_picks_by_list_id(DEFAULT_LIST_ID, Some(WALLET_A), 100, 0)
        .await
        .unwrap();
    assert_eq!(picks.len(), 1);
    assert_eq!(count, 1);

    scratch.drop().await;
}

/// Upstream `pickAndUnpickInBulk` runs the pick INSERT and the unpick DELETE
/// in one transaction: the pair lands together, and a failure in the second
/// statement rolls the first back.
#[tokio::test]
async fn a_bulk_pick_and_unpick_lands_as_one_write() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());
    let from = insert_list(&scratch.pool, "from", WALLET_A).await;
    let to = insert_list(&scratch.pool, "to", WALLET_A).await;
    lists
        .pick_in_lists(ITEM_X, WALLET_A, std::slice::from_ref(&from))
        .await
        .unwrap();

    lists
        .pick_and_unpick_in_bulk(
            ITEM_X,
            WALLET_A,
            std::slice::from_ref(&to),
            std::slice::from_ref(&from),
        )
        .await
        .unwrap();
    assert_eq!(picks_in(&scratch.pool, &from).await, 0);
    assert_eq!(picks_in(&scratch.pool, &to).await, 1);

    let failed = lists
        .pick_and_unpick_in_bulk(
            ITEM_Y,
            WALLET_A,
            std::slice::from_ref(&to),
            &["not-a-uuid".to_string()],
        )
        .await;
    assert!(
        failed.is_err(),
        "a DELETE that cannot bind must fail the call"
    );
    assert_eq!(
        picks_in(&scratch.pool, &to).await,
        1,
        "the INSERT that preceded the failed DELETE was rolled back"
    );

    scratch.drop().await;
}
