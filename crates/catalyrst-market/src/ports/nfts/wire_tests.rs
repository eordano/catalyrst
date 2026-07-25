use super::*;
use crate::ports::orders::Order;
use crate::ports::rentals::{RentalListing, RentalListingPeriod};

fn sample_db_nft() -> DbNft {
    DbNft {
        id: "0xparcel-7".into(),
        count: 1,
        contract_address: Some("0xparcel".into()),
        token_id: Some("7".into()),
        network: Some("ETHEREUM".into()),

        created_at: Some(1_700_000_000),
        url: None,
        updated_at: Some(1_700_000_500),
        sold_at: Some(0),
        urn: None,
        owner: Some("0xowner".into()),
        image: Some("img".into()),
        issued_id: None,
        item_id: None,
        item_type: None,
        rarity: None,
        category: Some("parcel".into()),
        name: Some("Parcel 1,2".into()),
        body_shapes: None,
        x: Some("1".into()),
        y: Some("2".into()),
        wearable_category: None,
        emote_category: None,
        description: None,
        size: None,
        subdomain: None,
        r#loop: None,
        has_sound: None,
        has_geometry: None,
        emote_outcome_type: None,
        estate_parcels: None,
        parcel_estate_token_id: None,
        parcel_estate_name: None,
        parcel_estate_id: None,
    }
}

fn sample_order() -> Order {
    Order {
        id: "order-1".into(),
        marketplace_address: "0xmkt".into(),
        contract_address: "0xparcel".into(),
        token_id: Some("7".into()),
        owner: "0xowner".into(),
        buyer: None,
        price: "1000000000000000000".into(),
        status: "open".into(),

        expires_at: 1_900_000_000,

        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_500_000,
        network: Network::Ethereum,
        chain_id: ethereum_chain_id(),
        issued_id: None,
        trade_id: None,
    }
}

fn sample_listing() -> RentalListing {
    RentalListing {
        id: "rental-1".into(),
        nft_id: "0xparcel-7".into(),
        category: "parcel".into(),
        search_text: "1,2".into(),
        network: "ETHEREUM".into(),
        chain_id: 1,
        expiration: 1_900_000_000_000,
        signature: "0xsig".into(),
        nonces: vec!["0".into()],
        token_id: "7".into(),
        contract_address: "0xparcel".into(),
        rental_contract_address: "0xrentals".into(),
        lessor: Some("0xowner".into()),
        tenant: None,
        status: "open".into(),
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_500_000,
        started_at: None,
        periods: vec![RentalListingPeriod {
            min_days: 1,
            max_days: 30,
            price_per_day: "1000".into(),
        }],
        target: "0x0".into(),
        rented_days: None,
    }
}

#[test]
fn nft_result_composes_order_and_rental() {
    let db = sample_db_nft();
    let order = sample_order();
    let listing = sample_listing();

    let mut nft = from_db_nft_to_nft(&db);
    nft.active_order_id = Some(order.id.clone());
    nft.open_rental_id = Some(listing.id.clone());
    let result = NftResult {
        nft,
        order: Some(serde_json::to_value(&order).unwrap()),
        rental: Some(serde_json::to_value(&listing).unwrap()),
    };

    let v = serde_json::to_value(&result).unwrap();
    let nft = &v["nft"];

    assert_eq!(nft["activeOrderId"], serde_json::json!("order-1"));
    assert_eq!(nft["openRentalId"], serde_json::json!("rental-1"));

    assert_eq!(v["order"]["id"], serde_json::json!("order-1"));

    assert_eq!(
        v["order"]["createdAt"],
        serde_json::json!(1_700_000_000_000i64)
    );
    assert_eq!(
        v["order"]["updatedAt"],
        serde_json::json!(1_700_000_500_000i64)
    );
    assert_eq!(v["order"]["expiresAt"], serde_json::json!(1_900_000_000i64));

    assert_eq!(v["rental"]["id"], serde_json::json!("rental-1"));
    assert_eq!(v["rental"]["periods"][0]["minDays"], serde_json::json!(1));

    assert_eq!(nft["createdAt"], serde_json::json!(1_700_000_000_000i64));
    assert_eq!(nft["updatedAt"], serde_json::json!(1_700_000_500_000i64));
}

#[test]
fn nfts_query_excludes_social_emotes_only_when_include_false() {
    fn outer_where_sql(include_social_emotes: Option<bool>) -> String {
        let f = NftFilters {
            include_social_emotes,
            ..Default::default()
        };
        let (sql, _binds) = build_nfts_query(&f, false);
        sql
    }

    const SOCIAL_EMOTE_EXCLUSION: &str = "emote.outcome_type IS NULL";

    assert!(
        outer_where_sql(Some(false)).contains(SOCIAL_EMOTE_EXCLUSION),
        "Some(false) should exclude social emotes"
    );
    assert!(!outer_where_sql(None).contains(SOCIAL_EMOTE_EXCLUSION));
    assert!(!outer_where_sql(Some(true)).contains(SOCIAL_EMOTE_EXCLUSION));
}

#[test]
fn outer_filters_move_the_limit_past_the_filter() {
    let on_sale = NftFilters {
        category: Some(crate::dcl_schemas::NftCategory::Ens),
        is_on_sale: Some(true),
        first: Some(24),
        ..Default::default()
    };
    let (sql, _) = build_nfts_query(&on_sale, false);
    let (inner, outer) = sql
        .split_once("FROM filtered_nft nft")
        .expect("query shape");
    assert!(
            !inner.contains("LIMIT"),
            "inner LIMIT before the on-sale filter returns empty pages while COUNT sees all rows: {inner}"
        );
    assert!(
        outer.contains("LIMIT"),
        "page limit must still apply: {outer}"
    );

    let plain = NftFilters {
        category: Some(crate::dcl_schemas::NftCategory::Ens),
        first: Some(24),
        ..Default::default()
    };
    let (sql, _) = build_nfts_query(&plain, false);
    let (inner, outer) = sql
        .split_once("FROM filtered_nft nft")
        .expect("query shape");
    assert!(
        inner.contains("LIMIT"),
        "unfiltered queries keep the cheap inner limit: {inner}"
    );
    assert!(!outer.contains("LIMIT"), "{outer}");
}

#[test]
fn nft_result_without_order_or_rental_is_null() {
    let nft = from_db_nft_to_nft(&sample_db_nft());
    let result = NftResult {
        nft,
        order: None,
        rental: None,
    };
    let v = serde_json::to_value(&result).unwrap();
    assert!(v["order"].is_null());
    assert!(v["rental"].is_null());
    assert!(v["nft"]["activeOrderId"].is_null());
    assert!(v["nft"]["openRentalId"].is_null());
}
