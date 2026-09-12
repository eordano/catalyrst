use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{signed_fetch_headers, test_wallet, Wallet};
use catalyrst_fed::{RateLimiter, Signed, TypedMessage};
use catalyrst_market::fed::market_domain;
use catalyrst_market::fed::messages::OrderCreate;
use catalyrst_market::fed::replay::Replay;
use catalyrst_market::ports::accounts::AccountsComponent;
use catalyrst_market::ports::activity::ActivityComponent;
use catalyrst_market::ports::analytics_day_data::AnalyticsDayDataComponent;
use catalyrst_market::ports::bids::BidsComponent;
use catalyrst_market::ports::catalog::CatalogComponent;
use catalyrst_market::ports::collections::CollectionsComponent;
use catalyrst_market::ports::contracts::ContractsComponent;
use catalyrst_market::ports::items::ItemsComponent;
use catalyrst_market::ports::lists::ListsComponent;
use catalyrst_market::ports::mana_rate::ManaUsdRateComponent;
use catalyrst_market::ports::nfts::NftsComponent;
use catalyrst_market::ports::orders::OrdersComponent;
use catalyrst_market::ports::owners::OwnersComponent;
use catalyrst_market::ports::prices::PricesComponent;
use catalyrst_market::ports::rankings::RankingsComponent;
use catalyrst_market::ports::sales::SalesComponent;
use catalyrst_market::ports::shop_catalog::ShopCatalogComponent;
use catalyrst_market::ports::stats::StatsComponent;
use catalyrst_market::ports::trades::TradesComponent;
use catalyrst_market::ports::trendings::TrendingsComponent;
use catalyrst_market::ports::usage_grants::UsageGrantsComponent;
use catalyrst_market::ports::user_assets::UserAssetsComponent;
use catalyrst_market::ports::volume::VolumeComponent;
use catalyrst_market::{AppState, AppStateInner};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const ADMIN_TOKEN: &str = "opt-market-admin";

async fn build_state(pool: PgPool) -> AppState {
    let sales = Arc::new(SalesComponent::new(pool.clone()));
    let bids = Arc::new(BidsComponent::new(pool.clone()));
    let orders = Arc::new(OrdersComponent::new(pool.clone()));
    let trades = Arc::new(TradesComponent::new(pool.clone(), false));
    let replay = Replay::new(pool.clone())
        .await
        .expect("federation replay state");
    Arc::new(AppStateInner {
        accounts: AccountsComponent::new(pool.clone()),
        activity: ActivityComponent::new(sales, bids, orders, trades),
        analytics_day_data: AnalyticsDayDataComponent::new(pool.clone()),
        bids: BidsComponent::new(pool.clone()),
        catalog: CatalogComponent::new(pool.clone()),
        collections: CollectionsComponent::new(pool.clone()),
        contracts: ContractsComponent::new(pool.clone()),
        items: ItemsComponent::new(pool.clone()),
        lists: ListsComponent::new(pool.clone()).with_write(pool.clone()),
        mana_usd_rate: ManaUsdRateComponent::new("http://127.0.0.1:9".into(), 0.02, 86400),
        nfts: NftsComponent::new(pool.clone()),
        orders: OrdersComponent::new(pool.clone()),
        owners: OwnersComponent::new(pool.clone()),
        prices: PricesComponent::new(pool.clone()),
        rankings: RankingsComponent::new(pool.clone()),
        sales: SalesComponent::new(pool.clone()),
        shop_catalog: ShopCatalogComponent::new(pool.clone()),
        stats: StatsComponent::new(pool.clone()),
        trades: TradesComponent::new(pool.clone(), false),
        trendings: TrendingsComponent::new(pool.clone()),
        user_assets: UserAssetsComponent::new(pool.clone(), false),
        usage_grants: UsageGrantsComponent::new(Some(pool.clone())),
        volume: VolumeComponent::new(AnalyticsDayDataComponent::new(pool.clone())),
        mv_trades_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        pool,
        replay,
        limiter: Arc::new(RateLimiter::new(120, Duration::from_secs(60))),
        domain: market_domain(),
        admin_token: Some(ADMIN_TOKEN.into()),
        trade_rpc: Default::default(),
        http: reqwest::Client::new(),
    })
}

#[tokio::test]
async fn trades_membership_probe_is_batch_scoped() {
    let Some(scratch) = ScratchDb::builder(PG_VAR, "opt_market")
        .schemas(["marketplace", "squid_marketplace", "favorites"])
        .build()
        .await
    else {
        return;
    };
    sqlx::raw_sql(include_str!("../migrations/0002_trades_schema.sql"))
        .execute(&scratch.pool)
        .await
        .expect("0002 applies");

    let n = 300usize;
    for i in 0..n {
        let sig = format!("0x{:064x}", i);
        sqlx::query(
            "INSERT INTO marketplace.trades \
                 (network, chain_id, signature, hashed_signature, checks, signer, type, \
                  expires_at, effective_since, contract, created_at) \
             VALUES ('MATIC', 137, $1, $2, '{}'::jsonb, $3, 'bid'::marketplace.trade_type, \
                     now(), now(), '', now())",
        )
        .bind(format!("0xsig{:061x}", i))
        .bind(&sig)
        .bind(format!("0x{:040x}", i))
        .execute(&scratch.pool)
        .await
        .unwrap();
    }

    let seeded_hits: Vec<String> = (0..4).map(|i| format!("0x{:064x}", i)).collect();
    let mut batch: Vec<String> = seeded_hits.clone();
    for j in 0..6 {
        batch.push(format!("0x{:064x}", 1000 + j));
    }

    let known = catalyrst_market::trades_sync::known_signatures_among(&scratch.pool, &batch)
        .await
        .expect("membership probe");

    assert_eq!(
        known.len(),
        4,
        "returns exactly the batch\u{2229}seeded intersection"
    );
    for s in &seeded_hits {
        assert!(known.contains(s), "hit {s} present");
    }
    for j in 0..6 {
        assert!(
            !known.contains(&format!("0x{:064x}", 1000 + j)),
            "absent-batch sig excluded"
        );
    }
    for i in 4..n {
        assert!(
            !known.contains(&format!("0x{:064x}", i)),
            "out-of-batch seeded sig {i} excluded"
        );
    }

    let empty = catalyrst_market::trades_sync::known_signatures_among(&scratch.pool, &[])
        .await
        .expect("empty batch probe");
    assert!(empty.is_empty());

    scratch.drop().await;
}

#[tokio::test]
async fn snapshot_hash_queries_run_concurrently() {
    let Some(scratch) = ScratchDb::builder(PG_VAR, "opt_market")
        .schemas(["marketplace", "squid_marketplace", "favorites"])
        .build()
        .await
    else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_federation.sql"))
        .await;

    let bids = ["0bbb", "1bbb"];
    let orders = ["0ord", "1ord", "2ord"];
    let trades = ["0trd", "1trd"];
    for h in bids {
        sqlx::query(
            "INSERT INTO market_bids_local \
                 (signature_hash, item_id, signer, price, expires_at, signed_at, message_payload, received_at) \
             VALUES ($1, 'i', 's', 1, 0, 0, '{}'::jsonb, 0)",
        )
        .bind(h)
        .execute(&scratch.pool)
        .await
        .unwrap();
    }
    for h in orders {
        sqlx::query(
            "INSERT INTO market_orders_local \
                 (signature_hash, item_id, signer, price, expires_at, signed_at, message_payload, received_at) \
             VALUES ($1, 'i', 's', 1, 0, 0, '{}'::jsonb, 0)",
        )
        .bind(h)
        .execute(&scratch.pool)
        .await
        .unwrap();
    }
    for (idx, h) in trades.iter().enumerate() {
        sqlx::query(
            "INSERT INTO market_trades_local \
                 (signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, message_payload, received_at) \
             VALUES ($1, $2, 'b', $3, 0, 0, '{}'::jsonb, 0)",
        )
        .bind(h)
        .bind(format!("ord{idx}"))
        .bind(format!("0x{:040x}", idx))
        .execute(&scratch.pool)
        .await
        .unwrap();
    }

    for t in [
        "market_bids_local",
        "market_orders_local",
        "market_trades_local",
    ] {
        scratch
            .apply_sql(&format!("ALTER TABLE {t} RENAME TO {t}_real;"))
            .await;
        scratch
            .apply_sql(&format!(
                "CREATE VIEW {t} AS SELECT r.* FROM {t}_real r CROSS JOIN (SELECT pg_sleep(0.4)) _d;"
            ))
            .await;
    }

    let state = build_state(scratch.pool.clone()).await;

    let t = Instant::now();
    let axum::Json(snap) =
        catalyrst_market::handlers::federation::snapshot(axum::extract::State(state))
            .await
            .unwrap();
    let elapsed = t.elapsed();

    let mut bid_sorted: Vec<&str> = bids.to_vec();
    bid_sorted.sort_unstable();
    let mut order_sorted: Vec<&str> = orders.to_vec();
    order_sorted.sort_unstable();
    let mut trade_sorted: Vec<&str> = trades.to_vec();
    trade_sorted.sort_unstable();
    let mut h = Sha256::new();
    for s in bid_sorted
        .iter()
        .chain(order_sorted.iter())
        .chain(trade_sorted.iter())
    {
        h.update(s.as_bytes());
    }
    let expected_hash = hex::encode(h.finalize());
    assert_eq!(snap.log_hash, expected_hash, "log_hash parity");
    assert_eq!(snap.latest_bids_seq, 2);
    assert_eq!(snap.latest_orders_seq, 3);
    assert_eq!(snap.latest_trades_seq, 2);
    assert_eq!(snap.domain, "DecentralandMarket");

    assert!(
        elapsed < Duration::from_millis(1250),
        "snapshot hash queries ran concurrently (elapsed {elapsed:?})"
    );

    scratch.drop().await;
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn envelope<T: TypedMessage>(wallet: &Wallet, message: T) -> Vec<u8> {
    let mut nonce = [0u8; 16];
    rand::rng().fill_bytes(&mut nonce);
    let mut signed = Signed {
        domain: market_domain(),
        message,
        nonce,
        signed_at: now_secs(),
        signature: String::new(),
    };
    let hash = signed.hash();
    signed.signature = wallet.sign_message(&hash).expect("sign envelope");
    serde_json::to_vec(&signed).expect("serialize envelope")
}

fn signed_headers(wallet: &Wallet, method: &str, path: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in signed_fetch_headers(wallet, method, path) {
        headers.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    headers
}

async fn create_wearable_squid_tables(pool: &PgPool, nft_sleep_secs: f64) {
    sqlx::query(
        "CREATE TABLE squid_marketplace.nft_real ( \
            id text, contract_address text, token_id numeric, network text, \
            created_at int8, updated_at int8, urn text, owner_address text, \
            image text, item_id text, item_type text, metadata_id text, \
            transferred_at int8 )",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE VIEW squid_marketplace.nft AS \
         SELECT r.* FROM squid_marketplace.nft_real r \
         CROSS JOIN (SELECT pg_sleep({nft_sleep_secs})) _d"
    )))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE squid_marketplace.metadata (id text, wearable_id text)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE squid_marketplace.wearable (id text, category text, rarity text, name text, description text)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE squid_marketplace.item (id text, price numeric)")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn user_wearables_assets_and_grants_overlap() {
    let Some(scratch) = ScratchDb::builder(PG_VAR, "opt_market")
        .schemas(["marketplace", "squid_marketplace", "favorites"])
        .build()
        .await
    else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_federation.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0007_usage_grants.sql"))
        .await;

    scratch
        .apply_sql("ALTER TABLE marketplace.usage_grants RENAME TO usage_grants_real;")
        .await;
    scratch
        .apply_sql(
            "CREATE VIEW marketplace.usage_grants AS \
             SELECT r.* FROM marketplace.usage_grants_real r \
             CROSS JOIN (SELECT pg_sleep(1.4)) _d;",
        )
        .await;

    create_wearable_squid_tables(&scratch.pool, 0.2).await;

    let owner = "0x00000000000000000000000000000000000000aa";
    sqlx::query(
        "INSERT INTO squid_marketplace.nft_real \
            (id, contract_address, token_id, network, created_at, updated_at, urn, \
             owner_address, image, item_id, item_type, metadata_id, transferred_at) \
         VALUES ('nft-1', '0xctr', 5, 'MATIC', 100, 100, 'urn:test:wear:1', $1, \
                 'img', 'item-1', 'wearable_v2', NULL, 100)",
    )
    .bind(owner)
    .execute(&scratch.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO marketplace.usage_grants_real \
            (grantee_address, urn, category, unlock_at, status) \
         VALUES ($1, 'urn:test:other:9', 'wearable', now() + interval '10 days', 'active')",
    )
    .bind(owner)
    .execute(&scratch.pool)
    .await
    .unwrap();

    let state = build_state(scratch.pool.clone()).await;

    let t = Instant::now();
    let axum::Json(resp) = catalyrst_market::handlers::user_assets::wearables::get_user_wearables(
        State(state),
        Path((owner.to_string(),)),
        Query(vec![]),
    )
    .await
    .unwrap();
    let elapsed = t.elapsed();

    assert!(resp.ok);
    assert_eq!(resp.data.total, 1);
    assert_eq!(resp.data.elements.len(), 1);
    let el = serde_json::to_value(&resp.data.elements[0]).unwrap();
    assert_eq!(el["tokenId"], "5");
    assert_eq!(el["category"], "eyewear");
    assert!(el.get("status").is_none(), "no lease status overlay");
    assert!(el.get("unlockAt").is_none(), "no unlockAt overlay");

    assert!(
        elapsed < Duration::from_millis(1750),
        "assets and grants ran concurrently (elapsed {elapsed:?})"
    );

    scratch.drop().await;
}

#[tokio::test]
async fn create_order_auth_reads_overlap_with_identical_verdicts() {
    let Some(scratch) = ScratchDb::builder(PG_VAR, "opt_market")
        .schemas(["marketplace", "squid_marketplace", "favorites"])
        .build()
        .await
    else {
        return;
    };
    scratch
        .apply_sql(include_str!("../migrations/0001_federation.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0007_usage_grants.sql"))
        .await;

    sqlx::query(
        "CREATE TABLE squid_marketplace.account (id text PRIMARY KEY, address text NOT NULL)",
    )
    .execute(&scratch.pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE squid_marketplace.nft_real (item_id text, owner_id text, urn text)")
        .execute(&scratch.pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE VIEW squid_marketplace.nft AS \
         SELECT r.* FROM squid_marketplace.nft_real r \
         CROSS JOIN (SELECT pg_sleep(0.6)) _d",
    )
    .execute(&scratch.pool)
    .await
    .unwrap();

    let owner = test_wallet(11);
    let not_owner = test_wallet(23);
    let item_id = "0x3333333333333333333333333333333333333333-0";

    sqlx::query("INSERT INTO squid_marketplace.account (id, address) VALUES ('acct-owner', $1)")
        .bind(owner.address())
        .execute(&scratch.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO squid_marketplace.nft_real (item_id, owner_id, urn) VALUES ($1, 'acct-owner', 'urn-order')",
    )
    .bind(item_id)
    .execute(&scratch.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO marketplace.usage_grants \
            (grantee_address, urn, category, unlock_at, status) \
         VALUES ($1, 'urn-unrelated', 'wearable', now() + interval '10 days', 'active')",
    )
    .bind(not_owner.address())
    .execute(&scratch.pool)
    .await
    .unwrap();

    let state = build_state(scratch.pool.clone()).await;

    async fn post_order(
        state: &AppState,
        wallet: &Wallet,
        item_id: &str,
    ) -> (StatusCode, serde_json::Value) {
        let headers = signed_headers(wallet, "post", "/v1/federation/order");
        let body = envelope(
            wallet,
            OrderCreate {
                item_id: item_id.into(),
                price: "3000000000000000000".into(),
                expires_at: now_secs() + 86_400,
                signed_at: now_secs(),
            },
        );
        let (code, json) = catalyrst_market::handlers::federation::create_order(
            State(state.clone()),
            headers,
            body.into(),
        )
        .await;
        (code, serde_json::to_value(&json.0).unwrap())
    }

    let t = Instant::now();
    let (code, body) = post_order(&state, &not_owner, item_id).await;
    let elapsed = t.elapsed();
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert_eq!(body["ok"], false);
    assert_eq!(
        body["message"],
        "signer does not own any NFT for this order's item_id"
    );
    assert!(
        elapsed < Duration::from_millis(1000),
        "auth reads ran concurrently (elapsed {elapsed:?})"
    );

    let (code, body) = post_order(&state, &owner, item_id).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(
        body["signature_hash"].as_str().map(|s| s.len()),
        Some(64),
        "authorized order returns a 64-hex signature hash"
    );

    scratch.drop().await;
}
