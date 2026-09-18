use super::*;

#[test]
fn credit_peg_is_ten_credits_per_usd() {
    assert_eq!(CREDIT_USD, "0.10");
    let v: f64 = CREDIT_USD.parse().unwrap();
    assert!((v - 0.10).abs() < 1e-12);
    assert_eq!((1.0 / v).round() as i64, 10);
}

#[test]
fn fresh_reading_not_stale() {
    assert!(!is_stale(1_000, 1_300, 300));
    assert!(!is_stale(1_000, 1_000, 300));
}

#[test]
fn old_reading_is_stale() {
    assert!(is_stale(1_000, 1_301, 300));
}

#[test]
fn future_reading_is_not_stale() {
    assert!(!is_stale(2_000, 1_000, 300));
}

#[test]
fn extreme_values_do_not_panic() {
    assert!(!is_stale(i64::MIN, i64::MIN, 300));
    assert!(is_stale(i64::MIN, i64::MAX, 300));
    assert!(!is_stale(i64::MAX, i64::MIN, 300));
}

#[test]
fn item_query_uses_both_collection_and_index() {
    let params = item_query_params("0x59a90bad9570ecd08895f132daf7b79696337f61", "0");
    assert_eq!(
        params,
        [
            (
                "contractAddress",
                "0x59a90bad9570ecd08895f132daf7b79696337f61"
            ),
            ("itemId", "0"),
        ]
    );
}

#[test]
fn json_as_i64_accepts_int_and_float() {
    assert_eq!(
        json_as_i64(&serde_json::json!(1_690_000_000_i64)),
        Some(1_690_000_000)
    );
    assert_eq!(
        json_as_i64(&serde_json::json!(1_690_000_000.0)),
        Some(1_690_000_000)
    );
    assert_eq!(json_as_i64(&serde_json::json!("nope")), None);
}

const TWO_POW_216: &str = "105312291668557186697918027683670432318895095400549111254310977536";

#[test]
fn token_matches_item_decodes_dcl_v2_encoding() {
    assert!(token_matches_item("2901", "0"));
    assert!(!token_matches_item("2901", "1"));
    let item1_issued7 = (alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
        + alloy_primitives::U256::from(7u64))
    .to_string();
    assert!(token_matches_item(&item1_issued7, "1"));
    assert!(!token_matches_item(&item1_issued7, "0"));
    assert!(!token_matches_item("", "0"));
    assert!(!token_matches_item("0x2901", "0"));
}

fn order(mkt: &str, token: &str, price: &str, status: &str, expires: i64) -> serde_json::Value {
    serde_json::json!({
        "marketplaceAddress": mkt,
        "tokenId": token,
        "price": price,
        "status": status,
        "expiresAt": expires,
    })
}

fn trade_order(
    trade_id: &str,
    token: &str,
    price: &str,
    status: &str,
    expires: i64,
) -> serde_json::Value {
    serde_json::json!({
        "id": trade_id,
        "tradeId": trade_id,
        "marketplaceAddress": "0x540fb08eDb56AaE562864B390542C97F562825BA",
        "tokenId": token,
        "price": price,
        "status": status,
        "expiresAt": expires,
    })
}

fn contract_order(contract: &str, mkt: &str, token: &str, price: &str) -> serde_json::Value {
    let mut o = order(mkt, token, price, "open", 9_999);
    o["contractAddress"] = serde_json::Value::String(contract.to_string());
    o
}

#[test]
fn listing_for_pair_only_looks_at_its_own_collection() {
    let mv2 = MARKETPLACE_V2_POLYGON;
    let item3 = (alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
        * alloy_primitives::U256::from(3u64))
    .to_string();
    let a = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let orders = vec![
        contract_order(b, mv2, &item3, "100"),
        contract_order(a, mv2, &item3, "300"),
        contract_order(a, mv2, "2901", "5"),
        contract_order(&a.to_ascii_uppercase(), mv2, &item3, "200"),
    ];
    let got = select_listing_for_pair(&orders, a, "3", 1_000, true).expect("a match");
    assert_eq!(
        v2_of(&got),
        (item3.as_str(), "200"),
        "cheapest of a's item-3 orders"
    );
    assert_eq!(
        select_listing_for_pair(&orders, b, "3", 1_000, true).map(|l| l.price_wei),
        Some("100".to_string())
    );
    assert_eq!(
        select_listing_for_pair(&orders, a, "0", 1_000, true).map(|l| l.price_wei),
        Some("5".to_string()),
        "token 2901 >> 216 is item 0"
    );
    assert_eq!(select_listing_for_pair(&orders, a, "9", 1_000, true), None);
    assert_eq!(
        select_listing_for_pair(
            &orders,
            "0xcccccccccccccccccccccccccccccccccccccccc",
            "3",
            1_000,
            true
        ),
        None
    );
}

#[test]
fn decimal_item_ids_only_reach_the_market() {
    assert!(is_decimal_item_id("0"));
    assert!(is_decimal_item_id(" 12 "));
    assert!(!is_decimal_item_id(""));
    assert!(!is_decimal_item_id("1a"));
    assert!(!is_decimal_item_id("-1"));
    assert!(!is_decimal_item_id(&"9".repeat(78)));
}

fn v2_of(l: &OpenListing) -> (&str, &str) {
    match &l.venue {
        ListingVenue::V2 { token_id } => (token_id, &l.price_wei),
        other => panic!("expected a V2 listing, got {other:?}"),
    }
}

#[test]
fn select_cheapest_listing_picks_cheapest_matching_marketplacev2() {
    let mv2 = MARKETPLACE_V2_POLYGON;
    let now = 1_000i64;
    let orders = vec![
        order(
            "0xa40b1d129b8906888720686f3a01921ddf37716f",
            "2460",
            "1",
            "open",
            9_999,
        ),
        order(
            mv2,
            &(alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
                + alloy_primitives::U256::from(3u64))
            .to_string(),
            "5",
            "open",
            9_999,
        ),
        order(mv2, "2900", "5", "open", 500),
        order(mv2, "2902", "20000000000000000", "open", 9_999),
        order(mv2, "2901", "10000000000000000", "open", 9_999),
    ];
    let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
    assert_eq!(v2_of(&got), ("2901", "10000000000000000"));
}

#[test]
fn select_cheapest_listing_accepts_trade_venue_pinned_by_trade_id() {
    let now = 1_000i64;
    let orders = vec![
        order(
            MARKETPLACE_V2_POLYGON,
            "2901",
            "20000000000000000",
            "open",
            9_999,
        ),
        trade_order(
            "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
            "2902",
            "10000000000000000",
            "open",
            9_999,
        ),
    ];
    let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
    assert_eq!(
        got.venue,
        ListingVenue::Trade {
            trade_id: "1bbe7d78-dd71-4cbe-9085-70d679d3ad11".into()
        }
    );
    assert_eq!(got.price_wei, "10000000000000000");
}

#[test]
fn price_tie_prefers_the_onchain_v2_listing_over_the_trade() {
    let now = 1_000i64;
    let orders = vec![
        trade_order(
            "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
            "2902",
            "10000000000000000",
            "open",
            9_999,
        ),
        order(
            MARKETPLACE_V2_POLYGON,
            "2901",
            "10000000000000000",
            "open",
            9_999,
        ),
    ];
    let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
    assert!(
        matches!(got.venue, ListingVenue::V2 { .. }),
        "tie must go to on-chain V2: {got:?}"
    );
}

#[test]
fn trades_are_excluded_when_the_caller_cannot_fulfil_them() {
    let now = 1_000i64;
    let orders = vec![trade_order(
        "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
        "2902",
        "10000000000000000",
        "open",
        9_999,
    )];
    assert!(select_cheapest_listing(&orders, "0", now, false).is_none());
}

#[test]
fn expired_or_closed_trades_are_not_candidates() {
    let now = 1_000i64;
    let expired = vec![trade_order("t-1", "2902", "1000", "open", 500)];
    assert!(select_cheapest_listing(&expired, "0", now, true).is_none());
    let sold = vec![trade_order("t-1", "2902", "1000", "sold", 9_999)];
    assert!(select_cheapest_listing(&sold, "0", now, true).is_none());
}

fn info(price_wei: &str) -> ItemInfo {
    ItemInfo {
        item_id: "0".into(),
        urn: "urn:decentraland:matic:collections-v2:0x59a9:0".into(),
        category: "wearable".into(),
        price_wei: price_wei.into(),
        contract_address: "0x59a90bad9570ecd08895f132daf7b79696337f61".into(),
        store_mintable: false,
    }
}

fn mintable_info(price_wei: &str) -> ItemInfo {
    ItemInfo {
        store_mintable: true,
        ..info(price_wei)
    }
}

#[test]
fn secondary_basis_is_the_selected_listing_not_the_mint_price() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "10000000000000000".into(),
    };
    let got = resolve_basis("secondary", &info("0"), Some(listing)).unwrap();
    assert_eq!(got.basis_wei, "10000000000000000");
    assert_eq!(
        got.kind,
        BasisKind::Secondary {
            token_id: "2901".into()
        }
    );
}

#[test]
fn secondary_without_listing_is_rejected_not_priced_at_mint() {
    let err = resolve_basis("secondary", &info("0"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    let err = resolve_basis("secondary", &info("2500000000000000000"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
}

#[test]
fn primary_basis_stays_the_mint_price() {
    let got = resolve_basis("primary", &mintable_info("2500000000000000000"), None).unwrap();
    assert_eq!(got.basis_wei, "2500000000000000000");
    assert_eq!(got.kind, BasisKind::Primary);
}

#[test]
fn primary_refuses_when_the_store_cannot_mint() {
    let err = resolve_basis("primary", &info("2500000000000000000"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "10000000000000000".into(),
    };
    let err = resolve_basis("primary", &info("2500000000000000000"), Some(listing)).unwrap_err();
    assert!(
        matches!(err, ApiError::Conflict(_)),
        "primary mode must not silently fall back to a listing: {err:?}"
    );
}

#[test]
fn unknown_mode_is_refused_not_defaulted_to_mint() {
    let err = resolve_basis("tertiary", &mintable_info("1"), None).unwrap_err();
    assert!(matches!(err, ApiError::Internal(_)), "got {err:?}");
    let err = resolve_basis("", &mintable_info("1"), None).unwrap_err();
    assert!(matches!(err, ApiError::Internal(_)), "got {err:?}");
}

#[test]
fn auto_picks_the_cheaper_listing_over_the_pricier_mint() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "10000000000000000".into(),
    };
    let got = resolve_basis("auto", &mintable_info("2500000000000000000"), Some(listing)).unwrap();
    assert_eq!(got.basis_wei, "10000000000000000");
    assert_eq!(
        got.kind,
        BasisKind::Secondary {
            token_id: "2901".into()
        }
    );
}

#[test]
fn auto_picks_the_cheaper_mint_over_the_pricier_listing() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "2500000000000000000".into(),
    };
    let got = resolve_basis("auto", &mintable_info("10000000000000000"), Some(listing)).unwrap();
    assert_eq!(got.basis_wei, "10000000000000000");
    assert_eq!(got.kind, BasisKind::Primary);
}

#[test]
fn auto_ignores_a_free_mint_and_charges_the_listing() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "1".into(),
    };
    let got = resolve_basis("auto", &mintable_info("0"), Some(listing)).unwrap();
    assert_eq!(got.basis_wei, "1");
    assert_eq!(
        got.kind,
        BasisKind::Secondary {
            token_id: "2901".into()
        }
    );
}

#[test]
fn auto_refuses_a_free_mint_with_no_listing() {
    let err = resolve_basis("auto", &mintable_info("0"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)));
}

#[test]
fn primary_refuses_a_free_mint() {
    let err = resolve_basis("primary", &mintable_info("0"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)));
}

#[test]
fn auto_price_tie_goes_to_the_listing() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "10000000000000000".into(),
    };
    let got = resolve_basis("auto", &mintable_info("10000000000000000"), Some(listing)).unwrap();
    assert_eq!(
        got.kind,
        BasisKind::Secondary {
            token_id: "2901".into()
        },
        "on a tie the existing listing is bought, not a new mint"
    );
}

#[test]
fn auto_uses_the_listing_when_the_store_cannot_mint_even_if_mint_looks_cheaper() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "2500000000000000000".into(),
    };
    let got = resolve_basis("auto", &info("10000000000000000"), Some(listing)).unwrap();
    assert_eq!(got.basis_wei, "2500000000000000000");
    assert!(matches!(got.kind, BasisKind::Secondary { .. }));
}

#[test]
fn auto_unparseable_mint_price_defers_to_the_listing() {
    let listing = OpenListing {
        venue: ListingVenue::V2 {
            token_id: "2901".into(),
        },
        price_wei: "10000000000000000".into(),
    };
    let got = resolve_basis("auto", &mintable_info("not-a-price"), Some(listing)).unwrap();
    assert!(matches!(got.kind, BasisKind::Secondary { .. }));
    assert!(!mint_undercuts_listing("not-a-price", "10000000000000000"));
    assert!(!mint_undercuts_listing("1", "garbage"));
    assert!(mint_undercuts_listing(" 1 ", "2"));
    assert!(!mint_undercuts_listing("2", "2"));
}

#[test]
fn auto_falls_back_to_the_store_mint_when_no_listing() {
    let got = resolve_basis("auto", &mintable_info("2500000000000000000"), None).unwrap();
    assert_eq!(got.basis_wei, "2500000000000000000");
    assert_eq!(got.kind, BasisKind::Primary);
}

#[test]
fn auto_refuses_when_neither_listing_nor_store_mint_exists() {
    let err = resolve_basis("auto", &info("2500000000000000000"), None).unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
}

#[test]
fn item_mintability_comes_from_is_on_sale_without_a_trade() {
    let mintable = |is_on_sale: bool, trade_id: serde_json::Value| {
        let on_sale = is_on_sale;
        let has_trade = trade_id.as_str().is_some_and(|s| !s.trim().is_empty());
        on_sale && !has_trade
    };
    assert!(mintable(true, serde_json::Value::Null));
    assert!(mintable(true, serde_json::json!("")));
    assert!(!mintable(true, serde_json::json!("df638de9-uuid")));
    assert!(!mintable(false, serde_json::Value::Null));
}

#[test]
fn quote_and_checkout_derive_the_same_basis_from_the_same_selection() {
    let now = 1_000i64;
    let orders = vec![
        order(
            MARKETPLACE_V2_POLYGON,
            "2902",
            "20000000000000000",
            "open",
            9_999,
        ),
        order(
            MARKETPLACE_V2_POLYGON,
            "2901",
            "10000000000000000",
            "open",
            9_999,
        ),
    ];
    let quote_side = resolve_basis(
        "secondary",
        &info("0"),
        select_cheapest_listing(&orders, "0", now, true),
    )
    .unwrap();
    let checkout_side = resolve_basis(
        "secondary",
        &info("0"),
        select_cheapest_listing(&orders, "0", now, true),
    )
    .unwrap();
    assert_eq!(quote_side, checkout_side);
    assert_eq!(quote_side.basis_wei, "10000000000000000");
}

#[test]
fn never_zero_guard_rejects_zero_charge_while_broker_pays() {
    let err = ensure_charge_covers_payment("10000000000000000", "0").unwrap_err();
    assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    assert!(ensure_charge_covers_payment("10000000000000000", "abc").is_err());
    assert!(ensure_charge_covers_payment("10000000000000000", "").is_err());
    assert!(ensure_charge_covers_payment("garbage", "0").is_err());
    assert!(ensure_charge_covers_payment("", "0").is_err());
    ensure_charge_covers_payment("10000000000000000", "1").unwrap();
    ensure_charge_covers_payment("0", "0").unwrap();
    ensure_charge_covers_payment("000", "0.00").unwrap();
    ensure_charge_covers_payment("0", "5").unwrap();
}

#[test]
fn charge_and_payment_positivity_parsers() {
    assert!(charge_is_positive("1"));
    assert!(charge_is_positive("0.5"));
    assert!(charge_is_positive(" 10 "));
    assert!(!charge_is_positive("0"));
    assert!(!charge_is_positive("0.000"));
    assert!(!charge_is_positive(""));
    assert!(!charge_is_positive("-1"));
    assert!(!charge_is_positive("1e3"));

    assert!(payment_is_positive("1"));
    assert!(payment_is_positive("10000000000000000"));
    assert!(payment_is_positive("nonsense"));
    assert!(payment_is_positive(""));
    assert!(!payment_is_positive("0"));
    assert!(!payment_is_positive("000"));
}

#[test]
fn select_cheapest_listing_none_when_no_matching_venue() {
    let now = 1_000i64;
    let orders = vec![order(
        "0xa40b1d129b8906888720686f3a01921ddf37716f",
        "2460",
        "1",
        "open",
        9_999,
    )];
    assert!(select_cheapest_listing(&orders, "0", now, true).is_none());
    let orders2 = vec![order(
        MARKETPLACE_V2_POLYGON,
        &(alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
            + alloy_primitives::U256::from(1u64))
        .to_string(),
        "1",
        "open",
        9_999,
    )];
    assert!(select_cheapest_listing(&orders2, "0", now, true).is_none());
}
