//! Override set/clear run as one statement each and the poller-published
//! snapshot is served without a DB read; these pin those behaviours.
//! DB-gated via `CATALYRST_TEST_PG`.

use std::str::FromStr;

use catalyrst_price::ports::overrides::OverridesComponent;
use catalyrst_price::ports::prices::{PriceSnapshot, PricesComponent};
use chrono::Utc;
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;

async fn scratch(name: &str) -> Option<(PgPool, String)> {
    let Ok(url) = std::env::var("CATALYRST_TEST_PG") else {
        eprintln!("CATALYRST_TEST_PG unset; skipping");
        return None;
    };
    let schema = format!("price_rt_{name}_{}", std::process::id());
    let admin = PgPool::connect(&url).await.expect("connect scratch pg");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let opts = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPool::connect_with(opts).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    Some((pool, schema))
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(pool)
        .await
        .unwrap();
}

async fn audit(pool: &PgPool) -> Vec<(String, Option<String>, String, serde_json::Value)> {
    sqlx::query_as(
        "SELECT action, value::text, admin, detail FROM price_override_audit \
         WHERE token_id = 'decentraland' AND vs_currency = 'usd' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn override_set_and_clear_write_the_audit_in_the_same_statement() {
    let Some((pool, schema)) = scratch("overrides").await else {
        return;
    };
    let overrides = OverridesComponent::new(pool.clone());

    let set = overrides
        .set("decentraland", "usd", "0.25", Some("manual"), "alice")
        .await
        .unwrap();
    assert_eq!(set.token_id, "decentraland");
    assert_eq!(set.value, "0.25");
    assert_eq!(set.note.as_deref(), Some("manual"));
    assert_eq!(set.updated_by.as_deref(), Some("alice"));

    let again = overrides
        .set("decentraland", "usd", "0.5", None, "bob")
        .await
        .unwrap();
    assert_eq!(again.value, "0.5");
    assert_eq!(again.note, None);
    assert_eq!(again.updated_by.as_deref(), Some("bob"));
    assert!(again.updated_at >= set.updated_at);
    let listed = overrides.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].value, "0.5");

    assert!(overrides
        .clear("decentraland", "usd", "carol")
        .await
        .unwrap());
    assert!(!overrides
        .clear("decentraland", "usd", "carol")
        .await
        .unwrap());
    assert!(overrides.list().await.unwrap().is_empty());

    let rows = audit(&pool).await;
    assert_eq!(
        rows.len(),
        3,
        "no audit row for a clear that removed nothing"
    );
    assert_eq!(rows[0].0, "override.set");
    assert_eq!(rows[0].1.as_deref(), Some("0.25"));
    assert_eq!(rows[0].2, "alice");
    assert_eq!(rows[0].3["note"], serde_json::json!("manual"));
    assert_eq!(rows[1].1.as_deref(), Some("0.5"));
    assert_eq!(rows[1].3["note"], serde_json::Value::Null);
    assert_eq!(rows[2].0, "override.clear");
    assert_eq!(rows[2].1, None);
    assert_eq!(rows[2].2, "carol");
    assert_eq!(
        rows[2].3,
        serde_json::json!({"token_id": "decentraland", "vs_currency": "usd"})
    );

    drop_schema(&pool, &schema).await;
}

async fn insert_row(pool: &PgPool, mana_usd: f64) {
    sqlx::query(
        "INSERT INTO price_snapshots (source, source_updated_at, mana_usd) \
         VALUES ('coingecko', now(), $1::double precision)",
    )
    .bind(mana_usd)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn live_snapshot_is_served_only_when_polling_is_on() {
    let Some((pool, schema)) = scratch("live").await else {
        return;
    };
    let live = PricesComponent::new(pool.clone(), true);
    let db_only = PricesComponent::new(pool.clone(), false);
    assert!(live.latest().await.unwrap().is_none());

    insert_row(&pool, 0.4).await;
    assert_eq!(live.latest().await.unwrap().unwrap().mana_usd, Some(0.4));
    assert_eq!(db_only.latest().await.unwrap().unwrap().mana_usd, Some(0.4));

    insert_row(&pool, 0.5).await;
    assert_eq!(
        live.latest().await.unwrap().unwrap().mana_usd,
        Some(0.4),
        "a cold-start DB read seeds the live cell; only the poller replaces it"
    );
    assert_eq!(db_only.latest().await.unwrap().unwrap().mana_usd, Some(0.5));

    live.publish(PriceSnapshot {
        mana_usd: Some(0.6),
        mana_eth: None,
        mana_btc: None,
        market_cap_usd: None,
        volume_24h_usd: None,
        price_change_24h_pct: None,
        source_updated_at: None,
        taken_at: Utc::now(),
    });
    assert_eq!(live.latest().await.unwrap().unwrap().mana_usd, Some(0.6));
    assert_eq!(db_only.latest().await.unwrap().unwrap().mana_usd, Some(0.5));

    drop_schema(&pool, &schema).await;
}
