// The mv_trades migration chain applied greenfield, then judged by what
// Postgres compiled and by the status it computes from real indexer-shaped rows
// (marketplace-server 0446723 + 668fd54).
//
// Greenfield means the schemas the chain assumes exist -- marketplace,
// favorites, squid_marketplace -- are created first and the connection runs
// with them on its search_path, exactly as the deployed role does; 0004's view
// reads squid_marketplace.nft/item, so the two are stubbed with the columns the
// views touch. squid_trades is provisioned by 0011 itself, so the
// contract-scoped branch is the one under test.
//
// Set CATALYRST_MARKET_TEST_PG to run; each test builds a throwaway database
// and drops it on the way out.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";

const MIGRATIONS: [&str; 12] = [
    include_str!("../migrations/0001_federation.sql"),
    include_str!("../migrations/0002_trades_schema.sql"),
    include_str!("../migrations/0003_admin_moderation.sql"),
    include_str!("../migrations/0004_mv_trades.sql"),
    include_str!("../migrations/0005_mv_trades_onchain_invalidation.sql"),
    include_str!("../migrations/0006_favorites_lists.sql"),
    include_str!("../migrations/0007_usage_grants.sql"),
    include_str!("../migrations/0008_usage_grants_collection.sql"),
    include_str!("../migrations/0009_wearable_last_seen.sql"),
    include_str!("../migrations/0010_favorites_shared_default_list.sql"),
    include_str!("../migrations/0011_squid_trades_v3_contract_scope.sql"),
    include_str!("../migrations/0012_mv_trades_v3_cancellation_semantics.sql"),
];

const SIGNER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const STRANGER: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const MARKETPLACE_V2: &str = "0xa40b1d129b8906888720686f3a01921ddf37716f";
const MARKETPLACE_V3: &str = "0x36fd1434a6c4b8ade80c9847c1d15033ce34488c";
const COLLECTION: &str = "0x1111111111111111111111111111111111111111";
const MANA: &str = "0x2222222222222222222222222222222222222222";

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
        let database = format!("cg_mkt_chain_{}_{}", std::process::id(), nanos);
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {}", database)))
            .execute(&admin)
            .await
            .unwrap_or_else(|e| panic!("CREATE DATABASE {database} failed: {e}"));
        let (base, _) = admin_url
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{PG_VAR} is not a postgres URL: {admin_url}"));
        let db_url = format!(
            "{}/{}?options=-c%20search_path%3Dmarketplace,squid_marketplace,favorites,public",
            base, database
        );
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&db_url)
            .await
            .unwrap_or_else(|e| panic!("connect to scratch database {database} failed: {e}"));
        for schema in ["marketplace", "squid_marketplace", "favorites"] {
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}", schema)))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::raw_sql(
            "CREATE TABLE squid_marketplace.nft (
                id text PRIMARY KEY, contract_address text, token_id numeric,
                owner_address text, category text, issued_id numeric, name text,
                item_blockchain_id numeric);
             CREATE TABLE squid_marketplace.item (
                id text PRIMARY KEY, collection_id text, blockchain_id numeric,
                creator text, available numeric);",
        )
        .execute(&pool)
        .await
        .expect("squid_marketplace stubs");
        for (i, migration) in MIGRATIONS.iter().enumerate() {
            sqlx::raw_sql(*migration)
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("migration {:04} failed greenfield: {e}", i + 1));
        }
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

    async fn view_definition(&self) -> String {
        sqlx::query_scalar::<_, String>("SELECT pg_get_viewdef('marketplace.mv_trades'::regclass)")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn insert_trade(&self, trade: &TradeRow) -> String {
        let checks = serde_json::json!({
            "uses": 1,
            "signerSignatureIndex": trade.signer_index,
            "contractSignatureIndex": trade.contract_index,
        });
        let id: String = sqlx::query_scalar(
            "INSERT INTO marketplace.trades \
                 (network, chain_id, signature, hashed_signature, trade_digest, checks, signer, \
                  type, expires_at, effective_since, contract) \
             VALUES ('MATIC', 137, $1, $2, $3, $4, $5, 'public_item_order', \
                     now() + interval '1 day', now() - interval '1 day', $6) \
             RETURNING id::text",
        )
        .bind(format!("0xsig{}", trade.hashed_signature))
        .bind(trade.hashed_signature)
        .bind(trade.trade_digest)
        .bind(checks)
        .bind(SIGNER)
        .bind(trade.contract)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        for (direction, asset_type, contract) in
            [("sent", 3i16, COLLECTION), ("received", 1i16, MANA)]
        {
            let asset_id: String = sqlx::query_scalar(
                "INSERT INTO marketplace.trade_assets \
                     (trade_id, direction, asset_type, contract_address, extra) \
                 VALUES ($1::uuid, $2::marketplace.asset_direction_type, $3, $4, '0x') \
                 RETURNING id::text",
            )
            .bind(&id)
            .bind(direction)
            .bind(asset_type)
            .bind(contract)
            .fetch_one(&self.pool)
            .await
            .unwrap();
            let child = if direction == "sent" {
                "INSERT INTO marketplace.trade_assets_item (asset_id, item_id) VALUES ($1::uuid, '7')"
            } else {
                "INSERT INTO marketplace.trade_assets_erc20 (asset_id, amount) VALUES ($1::uuid, 1000)"
            };
            sqlx::query(child)
                .bind(&asset_id)
                .execute(&self.pool)
                .await
                .unwrap();
        }
        id
    }

    async fn on_chain_action(&self, action: &str, signature: &str, caller: &str) {
        sqlx::query(
            "INSERT INTO squid_trades.trade \
                 (id, signature, trade_digest, network, action, caller, tx_hash) \
             VALUES ($1, $2, NULL, 'POLYGON', $3, $4, '0xtx')",
        )
        .bind(format!("{signature}-{caller}-{action}"))
        .bind(signature)
        .bind(action)
        .bind(caller)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn signature_index(&self, address: &str, contract: &str, index: i32) {
        sqlx::query(
            "INSERT INTO squid_trades.signature_index (id, address, contract, network, index) \
             VALUES ($1, $2, $3, 'POLYGON', $4)",
        )
        .bind(format!("{address}-{contract}-POLYGON"))
        .bind(address)
        .bind(contract)
        .bind(index)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn status_of(&self, trade_id: &str) -> String {
        sqlx::query("REFRESH MATERIALIZED VIEW marketplace.mv_trades")
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("SELECT status FROM marketplace.mv_trades WHERE id = $1::uuid")
            .bind(trade_id)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("trade {trade_id} is missing from mv_trades: {e}"))
            .try_get::<String, _>("status")
            .unwrap()
    }
}

struct TradeRow {
    hashed_signature: &'static str,
    trade_digest: Option<&'static str>,
    contract: &'static str,
    signer_index: i32,
    contract_index: i32,
}

fn v2_trade(hashed_signature: &'static str) -> TradeRow {
    TradeRow {
        hashed_signature,
        trade_digest: None,
        contract: MARKETPLACE_V2,
        signer_index: 0,
        contract_index: 0,
    }
}

#[tokio::test]
async fn the_compiled_definition_carries_the_upstream_invariants() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let definition = scratch.view_definition().await;
    assert!(
        definition.contains("lower(st.caller) = lower((t.signer)::text)"),
        "only the signer's own cancellation may count:\n{definition}"
    );
    assert!(
        definition.contains("st.signature = ANY (ARRAY[t.hashed_signature, t.trade_digest])"),
        "on-chain actions must match on either identifier:\n{definition}"
    );
    assert!(
        definition.contains("si_signer.contract = lower(t.contract)"),
        "the signer index must be scoped to the trade's marketplace:\n{definition}"
    );
    assert!(
        definition.contains("si_contract.address = lower(t.contract)")
            && definition.contains("si_contract.contract = lower(t.contract)"),
        "the contract index must be the trade's own marketplace holding its own counter:\n{definition}"
    );
    assert_eq!(
        definition.matches("'POLYGON'::text").count(),
        2,
        "both index joins translate MATIC to the indexer's POLYGON:\n{definition}"
    );
    assert!(
        !definition.contains("lower(si_signer.") && !definition.contains("lower(si_contract."),
        "indexer columns are written lowercase and compared raw:\n{definition}"
    );
    scratch.drop().await;
}

#[tokio::test]
async fn a_strangers_cancellation_leaves_the_trade_open() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let id = scratch.insert_trade(&v2_trade("0xh1")).await;
    scratch.on_chain_action("cancelled", "0xh1", STRANGER).await;
    assert_eq!(scratch.status_of(&id).await, "open");
    scratch.on_chain_action("cancelled", "0xh1", SIGNER).await;
    assert_eq!(scratch.status_of(&id).await, "cancelled");
    scratch.drop().await;
}

#[tokio::test]
async fn a_v3_cancellation_reaches_the_trade_through_its_digest() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let id = scratch
        .insert_trade(&TradeRow {
            hashed_signature: "0xh3",
            trade_digest: Some("0xd3"),
            contract: MARKETPLACE_V3,
            signer_index: 0,
            contract_index: 0,
        })
        .await;
    assert_eq!(scratch.status_of(&id).await, "open");
    scratch.on_chain_action("cancelled", "0xd3", SIGNER).await;
    assert_eq!(scratch.status_of(&id).await, "cancelled");
    scratch.drop().await;
}

#[tokio::test]
async fn a_signer_index_bump_only_cancels_trades_against_that_marketplace() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let id = scratch.insert_trade(&v2_trade("0xh4")).await;
    scratch.signature_index(SIGNER, MARKETPLACE_V3, 5).await;
    assert_eq!(
        scratch.status_of(&id).await,
        "open",
        "a counter held by another deployment says nothing about this trade"
    );
    scratch.signature_index(SIGNER, MARKETPLACE_V2, 5).await;
    assert_eq!(scratch.status_of(&id).await, "cancelled");
    scratch.drop().await;
}

#[tokio::test]
async fn the_contract_index_matches_the_marketplaces_own_counter_only() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let id = scratch.insert_trade(&v2_trade("0xh5")).await;
    scratch
        .signature_index(MARKETPLACE_V2, MARKETPLACE_V3, 9)
        .await;
    assert_eq!(
        scratch.status_of(&id).await,
        "open",
        "a row keyed to this marketplace but held by another must leave the trade open"
    );
    scratch
        .signature_index(MARKETPLACE_V2, MARKETPLACE_V2, 1)
        .await;
    assert_eq!(scratch.status_of(&id).await, "cancelled");
    scratch.drop().await;
}

#[tokio::test]
async fn one_row_per_trade_survives_a_signer_with_counters_on_several_deployments() {
    let Some(scratch) = Scratch::create().await else {
        return;
    };
    let id = scratch.insert_trade(&v2_trade("0xh6")).await;
    scratch.signature_index(SIGNER, MARKETPLACE_V2, 0).await;
    scratch.signature_index(SIGNER, MARKETPLACE_V3, 3).await;
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY marketplace.mv_trades")
        .execute(&scratch.pool)
        .await
        .expect("the unique index on (id) must hold with per-deployment counters");
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM marketplace.mv_trades WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&scratch.pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(scratch.status_of(&id).await, "open");
    scratch.drop().await;
}
