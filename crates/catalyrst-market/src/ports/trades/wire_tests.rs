use super::DbTradeListRow;
use chrono::TimeZone;

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn list_row_is_snake_case_with_iso_timestamps() {
    let ts = chrono::Utc
        .with_ymd_and_hms(2024, 3, 15, 12, 34, 56)
        .unwrap()
        + chrono::Duration::milliseconds(789);
    let row = DbTradeListRow {
        id: "abc".into(),
        chain_id: 137,
        checks: serde_json::json!({"uses": 1}),
        created_at: ts,
        effective_since: ts,
        expires_at: ts,
        network: "MATIC".into(),
        signature: "0xsig".into(),
        signer: "0xsigner".into(),
        trade_type: "public_nft_order".into(),
        contract: "0xcontract".into(),
    };
    let v = serde_json::to_value(&row).unwrap();
    let obj = v.as_object().unwrap();

    assert!(obj.contains_key("chain_id"));
    assert!(obj.contains_key("created_at"));
    assert!(obj.contains_key("effective_since"));
    assert!(obj.contains_key("expires_at"));

    assert!(!obj.contains_key("chainId"));
    assert!(!obj.contains_key("createdAt"));
    assert!(!obj.contains_key("effectiveSince"));
    assert!(!obj.contains_key("expiresAt"));

    assert_eq!(obj.get("type").unwrap(), "public_nft_order");

    assert_eq!(obj.get("created_at").unwrap(), "2024-03-15T12:34:56.789Z");
}

use super::{
    bid_accepted_event, item_sold_event, AssetMeta, DbTrade, Trade, TradeAsset, TradeView,
};

fn sample_view(trade_type: &str, sent: Vec<TradeAsset>, received: Vec<TradeAsset>) -> TradeView {
    let ts = chrono::Utc
        .with_ymd_and_hms(2024, 3, 15, 12, 34, 56)
        .unwrap()
        + chrono::Duration::milliseconds(789);
    TradeView {
        trade: DbTrade {
            id: "trade-1".into(),
            chain_id: 137,
            checks: serde_json::json!({"uses": 1, "signerSignatureIndex": 0}),
            created_at: ts,
            effective_since: ts,
            expires_at: ts,
            network: "MATIC".into(),
            signature: "0xsig".into(),
            signer: "0xsigner".into(),
            trade_type: trade_type.into(),
            contract: "0xcontract".into(),
        },
        sent,
        received,
    }
}

fn db_asset(
    asset_type: i32,
    direction: &str,
    amount: Option<&str>,
    token_id: Option<&str>,
    item_id: Option<&str>,
    beneficiary: Option<&str>,
) -> TradeAsset {
    TradeAsset {
        asset_type,
        contract_address: "0xasset".into(),
        beneficiary: beneficiary.map(String::from),
        direction: direction.into(),
        extra: "0xextra".into(),
        amount: amount.map(String::from),
        token_id: token_id.map(String::from),
        item_id: item_id.map(String::from),
    }
}

#[test]
fn public_trade_has_no_effective_since_or_expires_at() {
    let view = sample_view(
        "public_nft_order",
        vec![db_asset(1, "sent", Some("1000"), None, None, None)],
        vec![db_asset(
            3,
            "received",
            None,
            Some("42"),
            None,
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);
    let v = serde_json::to_value(&trade).unwrap();
    let obj = v.as_object().unwrap();

    assert!(obj.contains_key("chainId"));
    assert!(obj.contains_key("createdAt"));

    assert_eq!(
        obj.get("createdAt").unwrap(),
        &serde_json::json!(1_710_506_096_789i64)
    );

    assert!(!obj.contains_key("effectiveSince"));
    assert!(!obj.contains_key("effective_since"));
    assert!(!obj.contains_key("expiresAt"));
    assert!(!obj.contains_key("expires_at"));

    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "chainId",
            "checks",
            "contract",
            "createdAt",
            "id",
            "network",
            "received",
            "sent",
            "signature",
            "signer",
            "type"
        ]
    );
}

#[test]
fn detail_status_is_additive_and_only_present_when_known() {
    let view = sample_view(
        "public_nft_order",
        vec![db_asset(3, "sent", None, Some("42"), None, None)],
        vec![db_asset(
            1,
            "received",
            Some("1000"),
            None,
            None,
            Some("0xben"),
        )],
    );
    let mut trade = Trade::from_view(&view);
    assert!(serde_json::to_value(&trade)
        .unwrap()
        .get("status")
        .is_none());
    trade.status = Some("open".into());
    assert_eq!(
        serde_json::to_value(&trade).unwrap()["status"],
        serde_json::json!("open")
    );
}

#[test]
fn public_trade_assets_are_discriminated_union() {
    let view = sample_view(
        "bid",
        vec![db_asset(1, "sent", Some("1000"), None, None, None)],
        vec![db_asset(
            4,
            "received",
            None,
            None,
            Some("7"),
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);
    let v = serde_json::to_value(&trade).unwrap();

    let sent = &v["sent"][0];

    assert_eq!(sent["assetType"], serde_json::json!(1));
    assert_eq!(sent["amount"], serde_json::json!("1000"));
    assert!(sent.get("tokenId").is_none());
    assert!(sent.get("itemId").is_none());
    assert!(sent.get("beneficiary").is_none());
    assert!(sent.get("direction").is_none());
    assert_eq!(sent["contractAddress"], serde_json::json!("0xasset"));
    assert_eq!(sent["extra"], serde_json::json!("0xextra"));

    let received = &v["received"][0];

    assert_eq!(received["assetType"], serde_json::json!(4));
    assert_eq!(received["itemId"], serde_json::json!("7"));
    assert!(received.get("amount").is_none());
    assert!(received.get("tokenId").is_none());
    assert_eq!(received["beneficiary"], serde_json::json!("0xben"));
}

fn item_meta() -> AssetMeta {
    AssetMeta {
        image: "https://img/1.png".into(),
        seller: "0xcreator".into(),
        category: "wearable".into(),
        rarity: Some("mythic".into()),
        name: Some("Cool Hat".into()),
        contract_address: "0xcollection".into(),
        token_id: None,
        item_id: Some("7".into()),
    }
}

fn nft_meta() -> AssetMeta {
    AssetMeta {
        image: "https://img/42.png".into(),
        seller: "0xowner".into(),
        category: "wearable".into(),
        rarity: Some("epic".into()),
        name: Some("Rare Boots".into()),
        contract_address: "0xnftcontract".into(),
        token_id: Some("42".into()),
        item_id: None,
    }
}

#[test]
fn accept_event_bid_accepted_shape() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MARKETPLACE_BASE_URL", "https://market.example");

    let view = sample_view(
        "bid",
        vec![db_asset(
            1,
            "sent",
            Some("1500000000000000000"),
            None,
            None,
            None,
        )],
        vec![db_asset(
            4,
            "received",
            None,
            None,
            Some("7"),
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);
    let meta = item_meta();
    let ev = bid_accepted_event(&trade, &[&meta]).expect("event");

    assert_eq!(ev["type"], serde_json::json!("blockchain"));
    assert_eq!(ev["subType"], serde_json::json!("bid-accepted"));
    assert_eq!(ev["key"], serde_json::json!("bid-accepted-trade-1"));
    let md = &ev["metadata"];

    assert_eq!(md["address"], serde_json::json!("0xsigner"));
    assert_eq!(md["image"], serde_json::json!("https://img/1.png"));
    assert_eq!(md["seller"], serde_json::json!("0xcreator"));
    assert_eq!(md["category"], serde_json::json!("wearable"));
    assert_eq!(md["rarity"], serde_json::json!("mythic"));
    assert_eq!(
        md["link"],
        serde_json::json!("https://market.example/contracts/0xcollection/items/7")
    );
    assert_eq!(md["nftName"], serde_json::json!("Cool Hat"));
    assert_eq!(md["price"], serde_json::json!("1500000000000000000"));
    assert_eq!(md["title"], serde_json::json!("Bid Accepted"));
    assert_eq!(
        md["description"],
        serde_json::json!("Your bid for 1.5 MANA for this Cool Hat was accepted.")
    );
    assert_eq!(md["network"], serde_json::json!("MATIC"));

    assert!(md.get("buyer").is_none());
    assert!(md.get("tokenId").is_none());
}

#[test]
fn accept_event_item_sold_shape() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MARKETPLACE_BASE_URL", "https://market.example");

    let view = sample_view(
        "public_nft_order",
        vec![db_asset(3, "sent", None, Some("42"), None, None)],
        vec![db_asset(
            1,
            "received",
            Some("2000000000000000000"),
            None,
            None,
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);
    let meta = nft_meta();
    let ev = item_sold_event(&trade, &[&meta], "0xbuyer").expect("event");

    assert_eq!(ev["type"], serde_json::json!("blockchain"));
    assert_eq!(ev["subType"], serde_json::json!("item-sold"));
    assert_eq!(ev["key"], serde_json::json!("item-sold-trade-1"));
    let md = &ev["metadata"];
    assert_eq!(md["address"], serde_json::json!("0xsigner"));
    assert_eq!(md["image"], serde_json::json!("https://img/42.png"));
    assert_eq!(md["seller"], serde_json::json!("0xowner"));

    assert_eq!(md["buyer"], serde_json::json!("0xbuyer"));
    assert_eq!(md["category"], serde_json::json!("wearable"));
    assert_eq!(md["rarity"], serde_json::json!("epic"));
    assert_eq!(
        md["link"],
        serde_json::json!("https://market.example/contracts/0xnftcontract/tokens/42")
    );
    assert_eq!(md["nftName"], serde_json::json!("Rare Boots"));
    assert_eq!(md["title"], serde_json::json!("Item Sold"));
    assert_eq!(
        md["description"],
        serde_json::json!("Someone just bought your Rare Boots")
    );
    assert_eq!(md["network"], serde_json::json!("MATIC"));

    assert_eq!(md["tokenId"], serde_json::json!("42"));

    assert!(md.get("price").is_none());
}

#[test]
fn accept_event_requires_exactly_one_resolved_asset() {
    let view = sample_view(
        "public_nft_order",
        vec![db_asset(3, "sent", None, Some("42"), None, None)],
        vec![db_asset(
            1,
            "received",
            Some("1000"),
            None,
            None,
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);

    assert!(item_sold_event(&trade, &[], "0xbuyer").is_none());

    let m1 = nft_meta();
    let m2 = item_meta();
    assert!(item_sold_event(&trade, &[&m1, &m2], "0xbuyer").is_none());
}

#[test]
fn rarity_and_name_omitted_when_absent() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("MARKETPLACE_BASE_URL", "");
    let view = sample_view(
        "public_item_order",
        vec![db_asset(4, "sent", None, None, Some("9"), None)],
        vec![db_asset(
            1,
            "received",
            Some("1000"),
            None,
            None,
            Some("0xben"),
        )],
    );
    let trade = Trade::from_view(&view);
    let meta = AssetMeta {
        image: "img".into(),
        seller: "0xc".into(),
        category: "emote".into(),
        rarity: None,
        name: None,
        contract_address: "0xcollection".into(),
        token_id: None,
        item_id: Some("9".into()),
    };
    let ev = item_sold_event(&trade, &[&meta], "0xbuyer").expect("event");
    let md = ev["metadata"].as_object().unwrap();

    assert!(!md.contains_key("rarity"));
    assert!(!md.contains_key("nftName"));
}
