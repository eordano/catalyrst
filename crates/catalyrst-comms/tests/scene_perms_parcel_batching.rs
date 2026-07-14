//! Perf regression: `is_scene_owner_or_admin` must resolve parcel ownership for
//! a multi-parcel place in ONE round trip (a coordinate-array `unnest` join),
//! not one SELECT per coordinate. Also pins boolean equivalence across the
//! three ownership shapes (direct owner on the last coord, estate owner,
//! unowned).
//!
//! DB-gated via the shared scratch cluster; skips cleanly when unset.
//! MUST be a current-thread `#[tokio::test]` -- see `support`'s SQL-counter docs.

mod support;

use catalyrst_comms::scene_perms::is_scene_owner_or_admin;
use catalyrst_contract_gate::pg::ScratchSchema;

const N: usize = 20;

async fn setup() -> Option<ScratchSchema> {
    let scratch = ScratchSchema::create("CATALYRST_COMMS_TEST_PG", "cg_comms_sceneperms").await?;
    // scene_admin (probed first by is_scene_owner_or_admin).
    scratch
        .apply_sql(include_str!("../migrations/0001_comms.sql"))
        .await;
    // Minimal places/squid fixture tables, created unqualified so they land in
    // the scratch schema via search_path. `apply_sql` splits on lines ending in
    // `;`, so keep each CREATE TABLE to its own single-line statement.
    scratch
        .apply_sql(
            "CREATE TABLE place (id text PRIMARY KEY, raw jsonb NOT NULL, base_position text NOT NULL DEFAULT '0,0');",
        )
        .await;
    scratch
        .apply_sql(
            "CREATE TABLE nft (category text, id text, name text, owner_address text, search_parcel_x numeric, search_parcel_y numeric, search_parcel_estate_id text);",
        )
        .await;
    Some(scratch)
}

async fn insert_place(pool: &sqlx::PgPool, id: &str, y: usize) {
    let positions: Vec<String> = (0..N).map(|x| format!("{x},{y}")).collect();
    let raw = serde_json::json!({ "world": false, "positions": positions }).to_string();
    sqlx::query("INSERT INTO place (id, raw) VALUES ($1, $2::jsonb)")
        .bind(id)
        .bind(raw)
        .execute(pool)
        .await
        .expect("insert place");
}

async fn seed(pool: &sqlx::PgPool) {
    // p1: 20 parcels on row y=0; all owned by 0xother EXCEPT the last coord
    // (19,0), owned by a MIXED-CASE 0xOwNeR (exercises lower()). No estate.
    insert_place(pool, "p1", 0).await;
    let mut rows = Vec::new();
    for x in 0..N {
        let owner = if x == N - 1 { "0xOwNeR" } else { "0xother" };
        rows.push(format!("('parcel','p1-{x}','','{owner}',{x},0,NULL)"));
    }
    insert_place(pool, "p2", 1).await;
    for x in 0..N {
        rows.push(format!("('parcel','p2-{x}','','0xother',{x},1,'est1')"));
    }
    rows.push("('estate','est1','','0xestate',NULL,NULL,NULL)".to_string());
    let sql = format!(
        "INSERT INTO nft \
         (category, id, name, owner_address, search_parcel_x, search_parcel_y, search_parcel_estate_id) \
         VALUES {}",
        rows.join(", ")
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .expect("insert nft");
}

#[tokio::test]
async fn parcel_ownership_is_one_round_trip() {
    let Some(scratch) = setup().await else {
        return;
    };
    let pool = scratch.pool.clone();
    seed(&pool).await;

    let state = support::test_state(
        pool.clone(),
        Some(pool.clone()),
        Some(pool.clone()),
        scratch.schema.clone(),
    );

    let (counter, _guard) = support::install_sql_counter();

    assert!(
        is_scene_owner_or_admin(&state, "p1", "0xowner")
            .await
            .unwrap(),
        "signer owns the last parcel of p1"
    );
    assert_eq!(
        counter.count_containing(".nft"),
        1,
        "parcel ownership must be one round trip (direct-owner case)"
    );

    counter.reset();
    assert!(
        is_scene_owner_or_admin(&state, "p2", "0xestate")
            .await
            .unwrap(),
        "signer owns the estate covering p2"
    );
    assert_eq!(counter.count_containing(".nft"), 1, "estate-owner case");

    counter.reset();
    assert!(
        !is_scene_owner_or_admin(&state, "p1", "0xnobody")
            .await
            .unwrap(),
        "signer owns nothing"
    );
    assert_eq!(counter.count_containing(".nft"), 1, "unowned case");

    scratch.drop().await;
}
