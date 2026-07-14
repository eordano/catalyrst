use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode};
use catalyrst_contract_gate::{signed_fetch_headers, signed_fetch_headers_with, test_wallet};
use catalyrst_fed::RateLimiter;
use catalyrst_market::auth_chain::optional_signer;
use catalyrst_market::dcl_schemas::{ChainId, Network};
use catalyrst_market::fed::market_domain;
use catalyrst_market::fed::replay::Replay;
use catalyrst_market::http::response::{DataTotal, DataTotalString};
use catalyrst_market::ports::accounts::AccountsComponent;
use catalyrst_market::ports::activity::ActivityComponent;
use catalyrst_market::ports::analytics_day_data::AnalyticsDayDataComponent;
use catalyrst_market::ports::bids::BidsComponent;
use catalyrst_market::ports::catalog::{
    CatalogComponent, CatalogItem, ItemData, PickStats, WearableData,
};
use catalyrst_market::ports::collections::CollectionsComponent;
use catalyrst_market::ports::contracts::ContractsComponent;
use catalyrst_market::ports::items::ItemsComponent;
use catalyrst_market::ports::lists::ListsComponent;
use catalyrst_market::ports::mana_rate::ManaUsdRateComponent;
use catalyrst_market::ports::nfts::NftsComponent;
use catalyrst_market::ports::orders::{Order, OrdersComponent};
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
use catalyrst_market::{api_router, AppState, AppStateInner};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

const ORDERS_FIXTURE: &str = include_str!("fixtures/upstream/orders-first1.json");
const CATALOG_FIXTURE: &str = include_str!("fixtures/upstream/catalog-first1.json");
const ORDERS_URL: &str = "https://marketplace-api.decentraland.org/v1/orders?first=1";
const CATALOG_URL: &str = "https://marketplace-api.decentraland.org/v1/catalog?first=1";

fn skeleton(v: &Value) -> Value {
    match v {
        Value::Null => Value::String("null".into()),
        Value::Bool(_) => Value::String("bool".into()),
        Value::Number(_) => Value::String("number".into()),
        Value::String(_) => Value::String("string".into()),
        Value::Array(a) => Value::Array(a.iter().map(skeleton).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, x)| (k.clone(), skeleton(x))).collect())
        }
    }
}

fn diff(ours: &Value, fx: &Value, path: &str, errs: &mut Vec<String>) {
    if matches!(fx, Value::String(s) if s == "null") {
        return;
    }
    match (ours, fx) {
        (Value::Object(o), Value::Object(f)) => {
            for k in f.keys() {
                if !o.contains_key(k) {
                    errs.push(format!("{path}/{k}: missing key"));
                }
            }
            for k in o.keys() {
                if !f.contains_key(k) {
                    errs.push(format!("{path}/{k}: unexpected extra key"));
                }
            }
            for (k, fv) in f {
                if let Some(ov) = o.get(k) {
                    diff(ov, fv, &format!("{path}/{k}"), errs);
                }
            }
        }
        (Value::Array(o), Value::Array(f)) => {
            if let Some(rep) = f.first() {
                for (i, ov) in o.iter().enumerate() {
                    diff(ov, rep, &format!("{path}/{i}"), errs);
                }
            }
        }
        (Value::String(a), Value::String(b)) => {
            if a != b {
                errs.push(format!("{path}: got {a}, upstream {b}"));
            }
        }
        _ => errs.push(format!("{path}: shape mismatch {ours} vs {fx}")),
    }
}

fn assert_skeleton_matches(ours: &Value, fixture: &Value) {
    let mut errs = Vec::new();
    diff(&skeleton(ours), &skeleton(fixture), "", &mut errs);
    assert!(errs.is_empty(), "wire skeleton drift:\n{}", errs.join("\n"));
}

fn sample_order() -> Order {
    Order {
        id: "6d35034c-0618-49d5-8e97-84782e5878cd".into(),
        marketplace_address: "0x1b67d0e31eeb6b52d8eeed71d3616c2f5b33b8e7".into(),
        contract_address: "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d".into(),
        token_id: Some("115792089237316195423570985008687907809".into()),
        owner: "0xc723cf4771b5373e4a2f5cfd1d773a61c9d6e18d".into(),
        buyer: Some(String::new()),
        price: "1500000000000000000000".into(),
        status: "open".into(),
        expires_at: 1_789_617_600,
        created_at: 1_787_061_057,
        updated_at: 1_787_061_057,
        network: Network::Ethereum,
        chain_id: ChainId::EthereumMainnet,
        issued_id: None,
        trade_id: Some("6d35034c-0618-49d5-8e97-84782e5878cd".into()),
    }
}

fn sample_catalog_item() -> CatalogItem {
    CatalogItem {
        id: "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925-0".into(),
        beneficiary: Some("0xc723cf4771b5373e4a2f5cfd1d773a61c9d6e18d".into()),
        item_id: "0".into(),
        name: "sample".into(),
        thumbnail: "https://peer.decentraland.org/thumb".into(),
        url: "/contracts/0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925/items/0".into(),
        urn: "urn:decentraland:matic:collections-v2:0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925:0"
            .into(),
        category: "wearable",
        contract_address: "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925".into(),
        rarity: "epic".into(),
        available: 100,
        is_on_sale: true,
        trade_id: None,
        creator: "0xc723cf4771b5373e4a2f5cfd1d773a61c9d6e18d".into(),
        data: ItemData::Wearable {
            wearable: WearableData {
                description: Some(String::new()),
                category: Some("upper_body".into()),
                body_shapes: vec!["BaseMale".into()],
                rarity: "epic".into(),
                is_smart: false,
            },
        },
        network: Network::Matic,
        chain_id: ChainId::MaticMainnet,
        price: "0".into(),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
        reviewed_at: 1_700_000_000,
        first_listed_at: Some(1_700_000_000),
        sold_at: 0,
        min_price: Some("0".into()),
        max_listing_price: None,
        min_listing_price: None,
        listings: Some(0),
        owners: None,
        picks: Some(PickStats {
            count: 2,
            item_id: "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925-0".into(),
            picked_by_user: None,
        }),
    }
}

#[test]
fn orders_total_is_wire_string() {
    let ours = serde_json::to_value(DataTotalString {
        data: vec![sample_order()],
        total: "160047".into(),
    })
    .unwrap();
    let fixture: Value = serde_json::from_str(ORDERS_FIXTURE).unwrap();
    assert!(
        ours["total"].is_string(),
        "our /v1/orders total must be a JSON string"
    );
    assert!(
        fixture["total"].is_string(),
        "the pinned fixture must keep upstream's stringified total"
    );
    assert_skeleton_matches(&ours, &fixture);
}

#[test]
fn catalog_envelope_parity() {
    let ours = serde_json::to_value(DataTotal {
        data: vec![sample_catalog_item()],
        total: 5,
    })
    .unwrap();
    let fixture: Value = serde_json::from_str(CATALOG_FIXTURE).unwrap();
    assert!(
        ours["total"].is_number(),
        "catalog total must stay a JSON number"
    );
    assert!(
        fixture["total"].is_number(),
        "the pinned catalog fixture total is a number"
    );
    assert_skeleton_matches(&ours, &fixture);
    assert_eq!(
        skeleton(&ours["data"][0]["picks"]),
        json!({ "count": "number", "itemId": "string" })
    );
}

#[test]
fn picks_stats_wire() {
    let anon = serde_json::to_value(PickStats {
        count: 2,
        item_id: "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925-0".into(),
        picked_by_user: None,
    })
    .unwrap();
    assert!(
        anon.get("pickedByUser").is_none(),
        "anonymous picks must omit pickedByUser"
    );
    assert_eq!(
        anon,
        json!({ "count": 2, "itemId": "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925-0" })
    );

    let authed = serde_json::to_value(PickStats {
        count: 2,
        item_id: "0xc5aaaa47d4ed16932d9970352afbd5e0df5a4925-0".into(),
        picked_by_user: Some(true),
    })
    .unwrap();
    assert_eq!(authed["pickedByUser"], json!(true));
}

fn header_map(pairs: Vec<(String, String)>) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        let name = HeaderName::from_bytes(k.as_bytes()).unwrap();
        let val = HeaderValue::from_str(&v).unwrap();
        h.insert(name, val);
    }
    h
}

#[tokio::test]
async fn optional_signer_semantics() {
    let empty = HeaderMap::new();
    assert!(matches!(
        optional_signer(&empty, "get", "/v1/catalog").await,
        Ok(None)
    ));

    let wallet = test_wallet(7);
    let headers = header_map(signed_fetch_headers(&wallet, "get", "/v1/catalog"));
    let signer = optional_signer(&headers, "get", "/v1/catalog")
        .await
        .expect("valid signed-fetch must verify")
        .expect("a present chain yields a signer");
    assert_eq!(signer, signer.to_lowercase());
    assert_eq!(signer, wallet.address());

    let mut corrupt = signed_fetch_headers(&wallet, "get", "/v1/catalog");
    for pair in corrupt.iter_mut() {
        if pair.0 == "x-identity-auth-chain-0" {
            pair.1 = "not-a-valid-auth-link".into();
        }
    }
    assert!(optional_signer(&header_map(corrupt), "get", "/v1/catalog")
        .await
        .is_err());
}

fn lazy_state() -> AppState {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://u:p@127.0.0.1:1/market")
        .expect("lazy pool builds without connecting");
    let sales = Arc::new(SalesComponent::new(pool.clone()));
    let bids = Arc::new(BidsComponent::new(pool.clone()));
    let orders = Arc::new(OrdersComponent::new(pool.clone()));
    let trades = Arc::new(TradesComponent::new(pool.clone(), false));
    let replay = Replay::empty(pool.clone());
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
        admin_token: Some("parity-admin".into()),
        trade_rpc: Default::default(),
        http: reqwest::Client::new(),
    })
}

const READ_PATHS: &[&str] = &[
    "/v1/contracts",
    "/v1/collections",
    "/v1/accounts",
    "/v1/owners",
    "/v1/catalog",
    "/v2/catalog",
    "/v3/catalog/shop",
    "/v3/catalog/legacy",
    "/v3/catalog/unified",
    "/v3/catalog/items",
    "/v3/catalog/related",
    "/v3/catalog/creators",
    "/v3/catalog/importable",
    "/v1/nfts",
    "/v1/items",
    "/v1/users/0x0000000000000000000000000000000000000001/wearables",
    "/v1/users/0x0000000000000000000000000000000000000001/wearables/grouped",
    "/v1/users/0x0000000000000000000000000000000000000001/wearables/urn-token",
    "/v1/users/0x0000000000000000000000000000000000000001/emotes",
    "/v1/users/0x0000000000000000000000000000000000000001/emotes/grouped",
    "/v1/users/0x0000000000000000000000000000000000000001/emotes/urn-token",
    "/v1/users/0x0000000000000000000000000000000000000001/names",
    "/v1/users/0x0000000000000000000000000000000000000001/names/names-only",
    "/v1/orders",
    "/v1/bids",
    "/v1/sales",
    "/v1/prices",
    "/v1/trendings",
    "/v1/rankings/wearables/1d",
    "/v1/stats/prices/listed",
    "/v1/volume/1d",
    "/v1/trades",
    "/v1/trades/x",
    "/v1/trades/0xabc/accept",
];

#[tokio::test]
async fn read_surface_no_404() {
    let app = api_router().with_state(lazy_state());
    for path in READ_PATHS {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(*path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "registered read path {path} must not 404 in-process (a 404 in prod is a proxy allowlist gap)"
        );
    }

    let control = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/definitely-not-a-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(control.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore]
async fn live_upstream_matches_fixture_skeleton() {
    if std::env::var("MARKET_PARITY_LIVE").as_deref() != Ok("1") {
        return;
    }
    for (url, fixture) in [(ORDERS_URL, ORDERS_FIXTURE), (CATALOG_URL, CATALOG_FIXTURE)] {
        let live: Value = reqwest::get(url).await.unwrap().json().await.unwrap();
        let pinned: Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(
            skeleton(&live),
            skeleton(&pinned),
            "upstream {url} drifted from the checked-in fixture skeleton"
        );
    }
}

const MARKETPLACE_METADATA: &str = r#"{"signer":"dcl:marketplace","intent":"dcl:create-trade"}"#;

async fn signed_route_answer(
    method: &str,
    uri: &str,
    sign_path: &str,
    metadata: &str,
    body: Option<&str>,
) -> (StatusCode, String) {
    let app = api_router().with_state(lazy_state());
    let wallet = test_wallet(11);
    let mut req = Request::builder()
        .method(method.to_uppercase().as_str())
        .uri(uri);
    for (name, value) in signed_fetch_headers_with(&wallet, method, sign_path, metadata) {
        req = req.header(name.as_str(), value.as_str());
    }
    let req = match body {
        Some(raw) => req
            .header("content-type", "application/json")
            .body(Body::from(raw.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };

    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let message = parsed
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (status, message)
}

async fn trade_answer(metadata: &str) -> (StatusCode, String) {
    signed_route_answer(
        "post",
        "/v1/trades",
        "/v1/trades",
        metadata,
        Some("not json"),
    )
    .await
}

async fn activity_answer(metadata: &str) -> (StatusCode, String) {
    signed_route_answer("get", "/v1/activity", "/v1/activity", metadata, None).await
}

/// Upstream gates POST /v1/trades with
/// `validateAuthMetadata(['dcl:marketplace','dcl:builder'], 'dcl:create-trade')`
/// (routes.ts:122). The legacy payload lowercases the metadata before signing,
/// so every rejected spelling below still carries a VALID signature -- the gate
/// is the only layer that can tell them apart.
#[tokio::test]
async fn post_trades_enforces_the_upstream_auth_metadata_policy() {
    let (status, message) = trade_answer(MARKETPLACE_METADATA).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        message.starts_with("invalid trade body"),
        "canonical marketplace metadata must reach the body parser, got: {message}"
    );

    for metadata in [
        r#"{"signer":"decentraland-kernel-scene","intent":"dcl:create-trade"}"#,
        r#"{"signer":"Decentraland-Kernel-Scene","intent":"dcl:create-trade"}"#,
        r#"{"signer":"Dcl:Marketplace","intent":"dcl:create-trade"}"#,
        r#"{"intent":"dcl:create-trade"}"#,
        "{}",
    ] {
        let (status, message) = trade_answer(metadata).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{metadata}");
        assert_eq!(message, "Invalid auth signer", "{metadata}");
    }

    for metadata in [
        r#"{"signer":"dcl:marketplace"}"#,
        r#"{"signer":"dcl:builder","intent":"dcl:marketplace:add-pick"}"#,
        r#"{"signer":"dcl:marketplace","intent":"Dcl:Create-Trade"}"#,
    ] {
        let (status, message) = trade_answer(metadata).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{metadata}");
        assert_eq!(
            message, "Invalid auth intent to perform this operation",
            "{metadata}"
        );
    }
}

/// GET /v1/activity carries the same signer allow-list with no intent
/// (routes.ts:165), so a canonical signer reaches the handler's own
/// address check and a re-spelled one never does.
#[tokio::test]
async fn get_activity_enforces_the_upstream_auth_metadata_policy() {
    let (status, message) = activity_answer(r#"{"signer":"dcl:marketplace"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message, "Unauthorized",
        "a canonical signer must reach the missing-address check"
    );

    let (status, message) =
        activity_answer(r#"{"signer":"dcl:builder","intent":"anything at all"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message, "Unauthorized",
        "the route declares no intent, so upstream leaves that field alone"
    );

    for metadata in [
        r#"{"signer":"Decentraland-Kernel-Scene"}"#,
        r#"{"signer":"decentraland-kernel-scene"}"#,
        r#"{"signer":"Dcl:Marketplace"}"#,
        "{}",
    ] {
        let (status, message) = activity_answer(metadata).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{metadata}");
        assert_eq!(message, "Invalid auth signer", "{metadata}");
    }
}
