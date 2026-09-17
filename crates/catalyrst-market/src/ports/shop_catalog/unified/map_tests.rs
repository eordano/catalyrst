use super::*;

fn core(acquisition: &str) -> ListingCore {
    ListingCore {
        source: "legacy".to_string(),
        acquisition: acquisition.to_string(),
        trade_id: "row-id".to_string(),
        trade_type: "public_item_order".to_string(),
        contract_address: Some("0xc".to_string()),
        item_id: Some("7".to_string()),
        token_id: None,
        name: Some("hat".to_string()),
        image: None,
        rarity: Some("Rare".to_string()),
        item_type: Some("wearable_v2".to_string()),
        wearable_category: Some("hat".to_string()),
        emote_loop: None,
        gender: None,
        creator: Some("0xdead".to_string()),
        seller: None,
        issued_id: None,
        price_credits: 5,
        compare_at_credits: None,
        sale_ends_at: None,
        sale_units_left: None,
        coupon: None,
        mana_wei: Some("1000".to_string()),
        available: Some("3".to_string()),
        network: Some("MATIC".to_string()),
        created_at: 1_000,
    }
}

fn item_row(acquisition: &str) -> RelatedItemRow {
    RelatedItemRow {
        core: core(acquisition),
        listing_count: 2,
    }
}

fn coupon_value() -> JsonValue {
    serde_json::json!({
        "id": "8f1a6d0e-0000-4000-8000-000000000001",
        "signer": "0x1111111111111111111111111111111111111111",
        "couponManager": "0x3fd3056ee72a2a85e9392fab3a450e7736536081",
        "couponAddress": "0xc914507fe297b2dddd1232ac3a8903f1c125e794",
        "checks": { "uses": 10 },
        "discountType": 1,
        "discount": 300_000,
        "root": "0xfeed",
        "collections": ["0x4c09495cd2d4e3d3fa2808eb655d013de426157b"],
        "signature": "0xsig",
        "used": 2
    })
}

/// `price_credits` on this feed is already the sale price, so the gate is whether the list
/// price still STRICTLY beats it once both are whole credits. A coupon that rounds away
/// would otherwise advertise a discount the buyer cannot see.
#[test]
fn a_coupon_that_rounds_to_the_same_credit_price_is_not_a_sale() {
    let collection = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
    let same = unified_sale_fields(
        Some(coupon_value()),
        collection,
        12,
        Some(12),
        Some(1_700_000_000),
        Some(4),
    );
    assert_eq!(same.compare_at_credits, None);
    assert_eq!(same.sale_ends_at, None);
    assert_eq!(same.sale_units_left, None);
    assert!(same.coupon.is_none());

    let cheaper = unified_sale_fields(
        Some(coupon_value()),
        collection,
        9,
        Some(12),
        Some(1_700_000_000),
        Some(4),
    );
    assert_eq!(cheaper.compare_at_credits, Some(12));
    assert_eq!(cheaper.sale_ends_at, Some(1_700_000_000));
    assert_eq!(cheaper.sale_units_left, Some(4));
    assert!(cheaper.coupon.is_some());
}

/// A row the coupon join never reached keeps every sale field null, coupon-free branches
/// included.
#[test]
fn a_row_without_a_coupon_carries_no_sale_fields() {
    let fields = unified_sale_fields(None, "0xcollection", 9, Some(12), Some(1), Some(4));
    assert_eq!(fields.compare_at_credits, None);
    assert!(fields.coupon.is_none());
}

#[test]
fn store_rows_carry_acquisition_and_drop_the_tiebreak_trade_id() {
    let m = map_unified_listing(core("store"));
    assert_eq!(m.acquisition, "store");
    assert_eq!(m.trade_id, None, "a mint has no trade");
    assert_eq!(m.listing_type, "primary");
    assert_eq!(m.mana_wei.as_deref(), Some("1000"));
}

#[test]
fn trade_rows_keep_their_trade_id() {
    let m = map_unified_listing(core("trade"));
    assert_eq!(m.acquisition, "trade");
    assert_eq!(m.trade_id.as_deref(), Some("row-id"));
}

#[test]
fn item_feed_maps_store_rows_the_same_way_plus_listing_count() {
    let m = map_unified_item(item_row("store"));
    assert_eq!(m.acquisition, "store");
    assert_eq!(m.trade_id, None);
    assert_eq!(m.listing_count, 2);

    let t = map_unified_item(item_row("trade"));
    assert_eq!(t.trade_id.as_deref(), Some("row-id"));
}

/// Never give `emote_loop` a `skip_serializing_if`: the key has to reach the
/// wire even when it is null, because the client tells an emote from a
/// wearable by `emoteLoop !== null` and an absent key reads as a wearable.
fn assert_play_mode_on_the_wire(v: &serde_json::Value, column: Option<bool>) {
    assert_eq!(
        v.get("emoteLoop"),
        Some(&serde_json::json!(column)),
        "emoteLoop must be present, and null only for a non-emote: {v}"
    );
}

#[test]
fn emote_play_mode_keeps_plays_once_distinct_from_not_an_emote() {
    for column in [Some(true), Some(false), None] {
        let mut listing = core("trade");
        listing.emote_loop = column;
        let listing = map_unified_listing(listing);
        assert_eq!(listing.emote_loop, column);
        assert_play_mode_on_the_wire(&serde_json::to_value(listing).unwrap(), column);

        let mut item = item_row("trade");
        item.core.emote_loop = column;
        let item = map_unified_item(item);
        assert_eq!(item.emote_loop, column);
        assert_play_mode_on_the_wire(&serde_json::to_value(item).unwrap(), column);
    }
}

#[test]
fn unified_wire_shape_serializes_acquisition_and_null_trade_id() {
    let v = serde_json::to_value(map_unified_listing(core("store"))).unwrap();
    assert_eq!(v["acquisition"], "store");
    assert!(v["tradeId"].is_null(), "{v}");
    assert_eq!(v["source"], "legacy");
}

/// `TrendingItem` flattens `UnifiedItem`, which itself flattens `ShopSaleFields`: upstream
/// reaches the same object by spread, which cannot nest. Keep both flattens -- a rail card
/// whose sale block sank under a nested key would render not-on-sale beside an on-sale grid
/// tile for the same item.
#[test]
fn trending_rail_flattens_the_sale_block_to_the_top_level() {
    let mut row = item_row("trade");
    row.core.coupon = Some(coupon_value());
    row.core.contract_address = Some("0x4c09495cd2d4e3d3fa2808eb655d013de426157b".to_string());
    row.core.price_credits = 9;
    row.core.compare_at_credits = Some(12);
    row.core.sale_ends_at = Some(1_700_000_000);
    row.core.sale_units_left = Some(4);

    let v = serde_json::to_value(TrendingItem {
        item: map_unified_item(row),
        trending_sales: 3,
    })
    .unwrap();

    assert_eq!(v["compareAtCredits"], serde_json::json!(12));
    assert_eq!(v["saleEndsAt"], serde_json::json!(1_700_000_000));
    assert_eq!(v["saleUnitsLeft"], serde_json::json!(4));
    assert!(v["coupon"]["proof"].is_array(), "{v}");
    assert_eq!(v["priceCredits"], serde_json::json!(9));
    assert_eq!(v["trendingSales"], serde_json::json!(3));
    assert_eq!(v["listingCount"], serde_json::json!(2));
    assert!(
        v.get("sale").is_none(),
        "the flatten must not emit a nested `sale` key: {v}"
    );
    assert!(
        v.get("item").is_none(),
        "the flatten must not emit a nested `item` key: {v}"
    );
}
