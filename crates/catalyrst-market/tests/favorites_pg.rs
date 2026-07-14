// 1. The half-ported ACL model (HIGH): `pick_in_lists` deliberately lost the
//    `AND user_address = $2` owner filter (upstream lets any caller pick into
//    an editable list -- notably the shared default Wishlist and any ACL-less
//    list, which upstream's LEFT-JOIN editability quirk treats as editable),
//    so the compensating upstream invariant MUST hold: every picks read in
//    `get_lists` is scoped to the caller. A foreign pick must never move
//    another caller's itemsCount / preview / isItemInList.
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

use std::time::Duration;

use catalyrst_market::ports::lists::{
    GetListsOptions, ListSortBy, ListSortDirection, ListsComponent, DEFAULT_LIST_ID,
    DEFAULT_LIST_USER_ADDRESS,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const WALLET_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WALLET_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ITEM_X: &str = "0x1111111111111111111111111111111111111111-0";
const ITEM_Y: &str = "0x2222222222222222222222222222222222222222-0";

struct Scratch {
    pool: PgPool,
    database: String,
    admin_url: String,
}

impl Scratch {
    async fn create() -> Option<Self> {
        let admin_url = catalyrst_testgate::require_pg(PG_VAR)?;
        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&admin_url)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                return catalyrst_testgate::pg_unusable(
                    PG_VAR,
                    &format!("connect to {admin_url} failed: {e}"),
                )
            }
        };
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let database = format!("cg_mkt_fav_{}_{}", std::process::id(), nanos);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {}", database)))
            .execute(&admin)
            .await
            .unwrap_or_else(|e| panic!("CREATE DATABASE {database} failed: {e}"));
        let (base, _) = admin_url
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{PG_VAR} is not a postgres URL: {admin_url}"));
        let db_url = format!("{}/{}", base, database);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&db_url)
            .await
            .unwrap_or_else(|e| panic!("connect to scratch database {database} failed: {e}"));
        sqlx::query("CREATE SCHEMA favorites")
            .execute(&pool)
            .await
            .unwrap();
        // The real migration files (raw_sql handles 0010's DO $$ guard block).
        sqlx::raw_sql(include_str!("../migrations/0006_favorites_lists.sql"))
            .execute(&pool)
            .await
            .expect("0006 applies");
        sqlx::raw_sql(include_str!(
            "../migrations/0010_favorites_shared_default_list.sql"
        ))
        .execute(&pool)
        .await
        .expect("0010 applies");
        Some(Self {
            pool,
            database,
            admin_url,
        })
    }

    async fn drop(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&self.admin_url)
            .await
        {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE {} WITH (FORCE)",
                self.database
            )))
            .execute(&admin)
            .await;
        }
    }
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

/// The exact HIGH scenario from the review: wallet B picks into wallet A's
/// ACL-less private list (check_non_editable_lists does not flag it -- the
/// faithfully-ported upstream LEFT-JOIN quirk -- and pick_in_lists inserts
/// without an owner filter). Wallet A's GET /v1/lists must come back with
/// A's OWN counts unchanged: itemsCount 1, preview [X], isItemInList(Y) false.
#[tokio::test]
async fn foreign_pick_does_not_move_the_owners_counts_or_preview() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let lists = ListsComponent::new(scratch.pool.clone()).with_write(scratch.pool.clone());

    let list_id = insert_list(&scratch.pool, "summer fits", WALLET_A).await;
    lists
        .pick_in_lists(ITEM_X, WALLET_A, std::slice::from_ref(&list_id))
        .await
        .unwrap();

    // Reachability: B's editability check passes on the ACL-less list...
    let non_editable = lists
        .check_non_editable_lists(std::slice::from_ref(&list_id), WALLET_B)
        .await
        .unwrap();
    assert!(
        non_editable.is_empty(),
        "upstream quirk: an ACL-less list is editable by anyone"
    );
    // ...and the (owner-filter-free) insert goes through.
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
    let Some(scratch) = Scratch::create().await else {
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
    let Some(scratch) = Scratch::create().await else {
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
    let Some(scratch) = Scratch::create().await else {
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
    let Some(scratch) = Scratch::create().await else {
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
