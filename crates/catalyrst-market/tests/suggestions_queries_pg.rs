//! The two suggestions queries whose shape a string assertion cannot prove: they name squid
//! columns and bind arrays, so the only way to know they parse and bind is to run them.

use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_market::ports::suggestions::queries::{select_item_attributes, select_owned_among};
use sqlx::Row;

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const WALLET: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const STRANGER: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const COLLECTION: &str = "0x1111111111111111111111111111111111111111";

async fn build_scratch() -> Option<ScratchDb> {
    let scratch = ScratchDb::builder(PG_VAR, "cg_mkt_sugg")
        .schemas(["squid_marketplace"])
        .build()
        .await?;
    sqlx::raw_sql(
        "CREATE TABLE squid_marketplace.nft (
             id text PRIMARY KEY, item_id text, owner_address text);
         CREATE TABLE squid_marketplace.item (
             id text PRIMARY KEY, collection_id text, creator text, rarity text,
             item_type text, search_wearable_category text, search_emote_category text,
             price numeric);",
    )
    .execute(&scratch.pool)
    .await
    .expect("squid stubs");
    Some(scratch)
}

/// The rail must never offer a wallet something it already holds, and the profile cannot answer
/// that: a gift is not a purchase, so it never reaches the profile at all.
#[tokio::test]
async fn the_owned_check_finds_every_holding_and_only_this_wallets() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (id, item_id, owner_address) VALUES
           ('n1', $1, $3), ('n2', $2, $4), ('n3', $1, $3)",
    )
    .bind(format!("{COLLECTION}-1"))
    .bind(format!("{COLLECTION}-2"))
    .bind(WALLET)
    .bind(STRANGER)
    .execute(&scratch.pool)
    .await
    .expect("holdings insert");

    let candidates = vec![
        format!("{COLLECTION}-1"),
        format!("{COLLECTION}-2"),
        format!("{COLLECTION}-3"),
    ];
    let rows = sqlx::query(sqlx::AssertSqlSafe(select_owned_among()))
        .bind(WALLET)
        .bind(candidates)
        .fetch_all(&scratch.pool)
        .await
        .expect("owned_among runs");
    let owned: Vec<String> = rows
        .into_iter()
        .map(|row| row.get::<String, _>("item_id"))
        .collect();

    assert_eq!(
        owned,
        vec![format!("{COLLECTION}-1")],
        "two nft rows for one item must collapse, and a stranger's holding must not appear"
    );
    scratch.drop().await;
}

/// The band is compared against candidates the unified core already priced in credits, and the
/// affinity map is keyed on the lowercase tier names.
#[tokio::test]
async fn the_profile_attributes_arrive_in_credits_with_a_lowercase_rarity() {
    let Some(scratch) = build_scratch().await else {
        return;
    };
    sqlx::query(
        "INSERT INTO squid_marketplace.item
           (id, collection_id, creator, rarity, item_type, search_wearable_category, price)
         VALUES ($1, $2, '0xcreator', 'Rare', 'wearable_v2', 'hat', 2000000000000000000)",
    )
    .bind(format!("{COLLECTION}-1"))
    .bind(COLLECTION)
    .execute(&scratch.pool)
    .await
    .expect("item insert");

    let row = sqlx::query(sqlx::AssertSqlSafe(select_item_attributes()))
        .bind(vec![format!("{COLLECTION}-1")])
        .bind(0.25f64)
        .fetch_one(&scratch.pool)
        .await
        .expect("attributes query runs");

    assert_eq!(row.get::<String, _>("rarity"), "rare");
    assert_eq!(row.get::<String, _>("sub_category"), "wearable:hat");
    assert!(row.get::<bool, _>("is_wearable"));
    // 2 MANA at 0.25 USD/MANA is 0.5 USD, and credits are ten to the dollar.
    assert!((row.get::<f64, _>("price_credits") - 5.0).abs() < 1e-9);
    scratch.drop().await;
}
