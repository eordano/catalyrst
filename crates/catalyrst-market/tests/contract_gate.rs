use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_contract_gate::{
    signed_fetch_headers, signed_fetch_headers_with, test_wallet, Case, Gate, Wallet,
};
use catalyrst_fed::{RateLimiter, Signed, TypedMessage};
use catalyrst_market::fed::market_domain;
use catalyrst_market::fed::messages::{
    BidAccept, BidCancel, BidPlace, OrderCancel, OrderCreate, TradeRecord,
};
use catalyrst_market::fed::replay::Replay;
use catalyrst_market::ports::accounts::AccountsComponent;
use catalyrst_market::ports::activity::ActivityComponent;
use catalyrst_market::ports::analytics_day_data::AnalyticsDayDataComponent;
use catalyrst_market::ports::bids::BidsComponent;
use catalyrst_market::ports::catalog::CatalogComponent;
use catalyrst_market::ports::collections::CollectionsComponent;
use catalyrst_market::ports::contracts::ContractsComponent;
use catalyrst_market::ports::coupons::{CouponsComponent, RpcCouponChainReader};
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
use catalyrst_market::{api_router_with_spec, AppState, AppStateInner};
use rand::Rng;
use serde_json::json;
use sqlx::PgPool;

const PG_VAR: &str = "CATALYRST_MARKET_TEST_PG";
const ADMIN_TOKEN: &str = "cg-market-admin";
const PICK_ITEM: &str = "0x1111111111111111111111111111111111111111-0";
const BID_ITEM: &str = "0x2222222222222222222222222222222222222222-0";
const ORDER_ITEM: &str = "0x3333333333333333333333333333333333333333-0";
const COUPON_ID: &str = "4f0f8d9a-8f2c-4a15-9f6b-1c2d3e4f5a6b";

async fn seed_squid(pool: &PgPool, accept: &str, owner: &str) {
    sqlx::query(
        "CREATE TABLE squid_marketplace.item (id text PRIMARY KEY, collection_id text, blockchain_id numeric)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE squid_marketplace.account (id text PRIMARY KEY, address text NOT NULL)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE squid_marketplace.nft (item_id text, owner_id text, urn text)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO squid_marketplace.item (id, collection_id, blockchain_id) VALUES ($1, 'col', 0)")
        .bind(PICK_ITEM)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO squid_marketplace.account (id, address) VALUES ('acct-accept', $1), ('acct-owner', $2)")
        .bind(accept)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO squid_marketplace.nft (item_id, owner_id, urn) VALUES ($1, 'acct-accept', 'urn-bid'), ($2, 'acct-owner', 'urn-order')",
    )
    .bind(BID_ITEM)
    .bind(ORDER_ITEM)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_coupon(pool: &PgPool, signer: &str) {
    sqlx::query(
        "INSERT INTO marketplace.coupons (id, network, chain_id, signer, signature, hashed_signature, state_key, coupon_manager, coupon_address, checks, discount_type, discount_ppm, root, collections, effective_since, expires_at) VALUES ($1::uuid, 'MATIC', 80002, $2, '0xgate-coupon-signature', '0xgate-coupon-hashed', '0xgate-coupon-state-key', '0x6c956587d9fe70032781edcdc626310648575382', '0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9', $3::jsonb, 1, 100000, '0xbb275d33d9fbb90ff34fd53181283e8cdebb0dd8e764f5cb6852d976a4495fa9', ARRAY['0x1111111111111111111111111111111111111111'], now() - interval '1 hour', now() + interval '1 day')",
    )
    .bind(COUPON_ID)
    .bind(signer.to_lowercase())
    .bind(r#"{"uses":10,"expiration":0,"effective":0,"salt":"0x","contractSignatureIndex":0,"signerSignatureIndex":0,"allowedRoot":"0x0000000000000000000000000000000000000000000000000000000000000000","externalChecks":[]}"#)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_list(pool: &PgPool, user: &str) {
    sqlx::query("INSERT INTO favorites.lists (name, user_address, is_private) VALUES ('gate list', $1, false)")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
}

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
        coupons: CouponsComponent::new(
            pool.clone(),
            pool.clone(),
            Arc::new(RpcCouponChainReader::new(
                reqwest::Client::new(),
                Default::default(),
            )),
        ),
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
        suggestions: catalyrst_market::ports::suggestions::SuggestionsComponent::new(
            pool.clone(),
            1,
        ),
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

fn with_signed_for(mut case: Case, wallet: &Wallet, method: &str, sign_path: &str) -> Case {
    for (name, value) in signed_fetch_headers(wallet, method, sign_path) {
        case = case.header(&name, &value);
    }
    case
}

/// The signer /v1/activity demands (upstream routes.ts:165). Without it the route answers 400
/// at the metadata gate, masking every auth case asserted below.
const ACTIVITY_METADATA: &str = r#"{"signer":"dcl:marketplace"}"#;

fn with_signed_meta_for(
    mut case: Case,
    wallet: &Wallet,
    method: &str,
    sign_path: &str,
    metadata: &str,
) -> Case {
    for (name, value) in signed_fetch_headers_with(wallet, method, sign_path, metadata) {
        case = case.header(&name, &value);
    }
    case
}

#[tokio::test]
async fn every_spec_route_answers_its_contract() {
    let Some(scratch) = ScratchDb::builder(PG_VAR, "cg_market")
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
        .apply_sql(include_str!("../migrations/0003_admin_moderation.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0006_favorites_lists.sql"))
        .await;
    sqlx::raw_sql(include_str!(
        "../migrations/0010_favorites_shared_default_list.sql"
    ))
    .execute(&scratch.pool)
    .await
    .expect("0010 applies");
    scratch
        .apply_sql(include_str!("../migrations/0007_usage_grants.sql"))
        .await;
    scratch
        .apply_sql(include_str!("../migrations/0013_coupons.sql"))
        .await;

    let user = test_wallet(7);
    let bidder = test_wallet(9);
    let owner = test_wallet(11);
    let accepter = test_wallet(13);

    seed_squid(&scratch.pool, &accepter.address(), &owner.address()).await;
    seed_list(&scratch.pool, &user.address()).await;
    seed_coupon(&scratch.pool, &user.address()).await;

    let (router, spec) = api_router_with_spec();
    let state = build_state(scratch.pool.clone()).await;
    let app: Router = router.with_state(state);
    let mut gate = Gate::new(serde_json::to_value(&spec).unwrap());

    let now = now_secs();
    let bid_expires = now + 86_400;

    let bid_accept_body = envelope(
        &bidder,
        BidPlace {
            item_id: BID_ITEM.into(),
            price: "1000000000000000000".into(),
            expires_at: bid_expires,
            fingerprint: String::new(),
            signed_at: now,
        },
    );
    let accepted_bid = gate
        .hit(
            &app,
            Case::new("post", "/v1/federation/bid")
                .signed(&bidder)
                .body(bid_accept_body, "application/json"),
        )
        .await;
    let accepted_bid_hash = accepted_bid["signature_hash"].as_str().unwrap().to_string();

    let bid_cancel_body = envelope(
        &bidder,
        BidPlace {
            item_id: BID_ITEM.into(),
            price: "1000000000000000000".into(),
            expires_at: bid_expires,
            fingerprint: String::new(),
            signed_at: now,
        },
    );
    let cancel_bid = gate
        .hit(
            &app,
            Case::new("post", "/v1/federation/bid")
                .signed(&bidder)
                .body(bid_cancel_body, "application/json"),
        )
        .await;
    let cancel_bid_hash = cancel_bid["signature_hash"].as_str().unwrap().to_string();

    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid")
            .signed(&bidder)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        with_signed_for(
            Case::new("post", "/v1/federation/bid"),
            &bidder,
            "post",
            "/v1/federation/order",
        )
        .body(b"{}".to_vec(), "application/json")
        .expect(401),
    )
    .await;

    let cancel_body = envelope(
        &bidder,
        BidCancel {
            bid_signature_hash: cancel_bid_hash.clone(),
            signed_at: now_secs(),
        },
    );
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/cancel")
            .signed(&bidder)
            .body(cancel_body, "application/json"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/cancel")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/cancel")
            .signed(&bidder)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;

    let accept_body = envelope(
        &accepter,
        BidAccept {
            bid_signature_hash: accepted_bid_hash.clone(),
            signed_at: now_secs(),
        },
    );
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/accept")
            .signed(&accepter)
            .body(accept_body, "application/json"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/accept")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/bid/accept")
            .signed(&accepter)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;

    let order_trade_body = envelope(
        &owner,
        OrderCreate {
            item_id: ORDER_ITEM.into(),
            price: "3000000000000000000".into(),
            expires_at: bid_expires,
            signed_at: now_secs(),
        },
    );
    let trade_order = gate
        .hit(
            &app,
            Case::new("post", "/v1/federation/order")
                .signed(&owner)
                .body(order_trade_body, "application/json"),
        )
        .await;
    let trade_order_hash = trade_order["signature_hash"].as_str().unwrap().to_string();

    let order_cancel_body = envelope(
        &owner,
        OrderCreate {
            item_id: ORDER_ITEM.into(),
            price: "3000000000000000000".into(),
            expires_at: bid_expires,
            signed_at: now_secs(),
        },
    );
    let cancel_order = gate
        .hit(
            &app,
            Case::new("post", "/v1/federation/order")
                .signed(&owner)
                .body(order_cancel_body, "application/json"),
        )
        .await;
    let cancel_order_hash = cancel_order["signature_hash"].as_str().unwrap().to_string();

    gate.hit(
        &app,
        Case::new("post", "/v1/federation/order")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/order")
            .signed(&owner)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;

    let order_cancel = envelope(
        &owner,
        OrderCancel {
            order_signature_hash: cancel_order_hash.clone(),
            signed_at: now_secs(),
        },
    );
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/order/cancel")
            .signed(&owner)
            .body(order_cancel, "application/json"),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/order/cancel")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/order/cancel")
            .signed(&owner)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;

    let trade_body = envelope(
        &owner,
        TradeRecord {
            order_signature_hash: trade_order_hash.clone(),
            buyer: bidder.address(),
            taken_at: now_secs(),
            tx_hash: "0xabcabcabcabcabcabcabcabcabcabcabcabcabca".into(),
            signed_at: now_secs(),
        },
    );
    let recorded_trade = gate
        .hit(
            &app,
            Case::new("post", "/v1/federation/trade")
                .signed(&owner)
                .body(trade_body, "application/json"),
        )
        .await;
    let trade_hash = recorded_trade["signature_hash"]
        .as_str()
        .unwrap()
        .to_string();
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/trade")
            .body(b"{}".to_vec(), "application/json")
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/federation/trade")
            .signed(&owner)
            .body(b"not json".to_vec(), "application/json")
            .expect(400),
    )
    .await;

    gate.hit(&app, Case::new("get", "/v1/federation/bids"))
        .await;
    gate.hit(&app, Case::new("get", "/v1/federation/orders"))
        .await;
    gate.hit(&app, Case::new("get", "/v1/federation/trades"))
        .await;
    gate.hit(&app, Case::new("get", "/federation/market/snapshot"))
        .await;
    gate.hit(&app, Case::new("get", "/federation/market/changes"))
        .await;

    gate.hit(&app, Case::new("get", "/v1/lists").signed(&user))
        .await;
    gate.hit(&app, Case::new("get", "/v1/lists").expect(401))
        .await;
    gate.hit(
        &app,
        with_signed_for(Case::new("get", "/v1/lists"), &user, "get", "/v1/other").expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/lists")
            .signed(&user)
            .query("sortBy=bogus")
            .expect(400),
    )
    .await;

    let list_picks_path = format!(
        "/v1/lists/{}/picks",
        catalyrst_market::ports::lists::DEFAULT_LIST_ID
    );
    gate.hit(
        &app,
        Case::new("get", "/v1/lists/{id}/picks").path(&list_picks_path),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/lists/{id}/picks")
            .path(&list_picks_path)
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        with_signed_for(
            Case::new("get", "/v1/lists/{id}/picks").path(&list_picks_path),
            &user,
            "get",
            "/v1/other",
        )
        .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/lists/{id}/picks")
            .path("/v1/lists/not-a-uuid/picks")
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        with_signed_meta_for(
            Case::new("get", "/v1/activity"),
            &user,
            "get",
            "/v1/other",
            ACTIVITY_METADATA,
        )
        .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/activity")
            .signed_meta(&user, &json!({ "signer": "dcl:marketplace" }))
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/activity")
            .signed_meta(&user, &json!({ "signer": "Decentraland-Kernel-Scene" }))
            .expect(400),
    )
    .await;
    gate.waive_success(
        "get",
        "/v1/activity",
        "a 200 aggregates the squid sales/bids/orders analytical tables plus the marketplace.trades / mv_trades reader (~10 subgraph-synced tables and materialized views) which the federation-scoped scratch database does not reproduce; unsigned->401, wrong-path->401, refused-signer->400 and missing-address->400 auth cases are asserted",
    );

    let pick_path = format!("/v1/picks/{}", PICK_ITEM);
    gate.hit(
        &app,
        Case::new("post", "/v1/picks/{item_id}")
            .path(&pick_path)
            .signed(&user)
            .json(&json!({})),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/picks/{item_id}")
            .path(&pick_path)
            .json(&json!({}))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/picks/{item_id}")
            .path(&pick_path)
            .signed(&user)
            .json(&json!({ "pickedFor": ["not-a-uuid"] }))
            .expect(400),
    )
    .await;
    gate.hit(
        &app,
        with_signed_for(
            Case::new("post", "/v1/picks/{item_id}").path(&pick_path),
            &user,
            "post",
            "/v1/picks/other",
        )
        .json(&json!({}))
        .expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("delete", "/v1/picks/{item_id}")
            .path(&pick_path)
            .signed(&user),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/picks/{item_id}")
            .path(&pick_path)
            .expect(401),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/v1/picks/stats").query(&format!("itemId={}", PICK_ITEM)),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/picks/stats").query(&format!(
            "itemId={}&checkingUserAddress={}",
            PICK_ITEM,
            user.address()
        )),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/picks/stats")
            .query("itemId=0xabc-0&checkingUserAddress=not-an-address")
            .expect(400),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/v1/coupons").query(&format!("signer={}", user.address())),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/coupons")
            .query("signer=not-an-address")
            .expect(400),
    )
    .await;

    let coupon_path = format!("/v1/coupons/{}", COUPON_ID);
    gate.hit(
        &app,
        Case::new("get", "/v1/coupons/{id}").path(&coupon_path),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/coupons/{id}")
            .path("/v1/coupons/not-a-uuid")
            .expect(404),
    )
    .await;

    gate.hit(
        &app,
        Case::new("post", "/v1/coupons")
            .json(&json!({}))
            .expect(401),
    )
    .await;
    gate.hit(
        &app,
        with_signed_meta_for(
            Case::new("post", "/v1/coupons"),
            &user,
            "post",
            "/v1/coupons",
            r#"{"signer":"dcl:marketplace","intent":"dcl:create-trade"}"#,
        )
        .json(&json!({}))
        .expect(400),
    )
    .await;
    gate.waive_success(
        "post",
        "/v1/coupons",
        "a 201 needs an EIP-712 CouponManager signature over a live chain id, a squid_marketplace.collection row whose creator is the recovered signer, and reachable CouponManager RPC for the signature indexes; none of those exist in the federation-scoped scratch database. The signature, ordering and persistence path is covered by the ports::coupons unit tests and by tests/coupons_pg.rs; unsigned->401 and wrong-intent->400 auth cases are asserted here",
    );

    gate.hit(
        &app,
        Case::new("get", "/v1/admin/moderation/flags").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("get", "/v1/admin/moderation/flags").expect(403),
    )
    .await;

    let flag_path = format!("/v1/admin/moderation/bid/{}/flag", accepted_bid_hash);
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/moderation/{kind}/{hash}/flag")
            .path(&flag_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "severity": "review", "reason": "gate" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/moderation/{kind}/{hash}/flag")
            .path(&flag_path)
            .json(&json!({ "severity": "review" }))
            .expect(403),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/admin/moderation/{kind}/{hash}/flag")
            .path(&flag_path)
            .bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(
        &app,
        Case::new("delete", "/v1/admin/moderation/{kind}/{hash}/flag")
            .path(&flag_path)
            .expect(403),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/v1/admin/disputes").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(&app, Case::new("get", "/v1/admin/disputes").expect(403))
        .await;

    let open_path = format!("/v1/admin/disputes/{}/open", trade_hash);
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/disputes/{trade_hash}/open")
            .path(&open_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "reason": "gate" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/disputes/{trade_hash}/open")
            .path(&open_path)
            .json(&json!({ "reason": "gate" }))
            .expect(403),
    )
    .await;

    let resolve_path = format!("/v1/admin/disputes/{}/resolve", trade_hash);
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/disputes/{trade_hash}/resolve")
            .path(&resolve_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "status": "resolved", "resolution": "ok" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/disputes/{trade_hash}/resolve")
            .path(&resolve_path)
            .json(&json!({ "status": "resolved" }))
            .expect(403),
    )
    .await;

    let force_path = format!(
        "/v1/admin/listings/order/{}/force-cancel",
        cancel_order_hash
    );
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/listings/{kind}/{hash}/force-cancel")
            .path(&force_path)
            .bearer(ADMIN_TOKEN)
            .json(&json!({ "reason": "gate" })),
    )
    .await;
    gate.hit(
        &app,
        Case::new("post", "/v1/admin/listings/{kind}/{hash}/force-cancel")
            .path(&force_path)
            .json(&json!({ "reason": "gate" }))
            .expect(403),
    )
    .await;

    gate.hit(
        &app,
        Case::new("get", "/v1/admin/audit").bearer(ADMIN_TOKEN),
    )
    .await;
    gate.hit(&app, Case::new("get", "/v1/admin/audit").expect(403))
        .await;

    gate.assert_covered();

    scratch.drop().await;
}
