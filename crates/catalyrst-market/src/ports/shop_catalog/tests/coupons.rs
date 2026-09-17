use super::super::component::{shop_sale_fields, to_shop_coupon};
use super::collections::UNION_BRANCHES;
use super::*;
use crate::ports::shop_catalog::types::SHOP_SORT_VALUES;

fn shop_sql(discounted: Option<bool>) -> String {
    build_shop_listings_sql(&ShopCatalogFilters {
        discounted,
        ..Default::default()
    })
    .0
}

fn unified_sql(discounted: Option<bool>) -> String {
    build_unified_listings_sql(
        &UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                discounted,
                ..Default::default()
            },
            ..Default::default()
        },
        0.5,
    )
    .0
}

/// Upstream a0580d9: every row of the plain shop feed is a primary listing already, so the
/// coupon join is unconditional there.
#[test]
fn the_shop_feed_always_joins_coupons() {
    let sql = shop_sql(None);
    assert_eq!(occurrences(&sql, "FROM marketplace.coupons c"), 1, "{sql}");
    assert!(
        sql.contains("LEFT JOIN marketplace.coupon_state cs"),
        "{sql}"
    );
    assert!(sql.contains("mv.type = 'public_item_order'"), "{sql}");
    assert!(
        sql.contains("LOWER(mv.sent_contract_address) = ANY(c.collections)"),
        "{sql}"
    );
    assert!(
        sql.contains("ORDER BY c.discount_ppm DESC, c.expires_at ASC"),
        "biggest discount wins, ties to the one ending soonest: {sql}"
    );
    assert!(sql.contains("LIMIT 1"), "{sql}");
}

/// `network_for_chain` collapses Polygon (137) and Amoy (80002) to the same `MATIC`, so
/// matching on the network alone lets a testnet coupon discount a mainnet listing in a database
/// carrying both -- and the buyer is then handed a coupon no CouponManager on that chain will
/// redeem. Every feed carrying the join must scope by chain.
#[test]
fn the_coupon_join_is_scoped_to_the_listings_own_chain() {
    for sql in [shop_sql(None), unified_sql(None)] {
        let joins = occurrences(&sql, "FROM marketplace.coupons c");
        assert_eq!(
            occurrences(&sql, "AND c.chain_id = mv.chain_id"),
            joins,
            "{sql}"
        );
        assert_eq!(
            occurrences(&sql, "AND c.network = mv.network"),
            joins,
            "{sql}"
        );
    }
}

#[test]
fn the_shop_feed_exposes_the_sale_price_and_the_coupon() {
    let sql = shop_sql(None);
    assert!(
        sql.contains("CASE WHEN cp.id IS NOT NULL THEN (mv.amount_received::numeric - FLOOR(mv.amount_received::numeric * cp.discount_ppm / 1000000))::text END AS sale_price"),
        "{sql}"
    );
    assert!(sql.contains("AS sale_ends_at"), "{sql}");
    assert!(sql.contains("AS sale_units_left"), "{sql}");
    assert!(sql.contains("AS coupon_discount_ppm"), "{sql}");
    assert!(sql.contains("jsonb_build_object("), "{sql}");
    assert!(sql.contains("'couponAddress', cp.coupon_address"), "{sql}");
    assert!(sql.contains("'discount', cp.discount_ppm"), "{sql}");
}

/// Read as a string, never as a presence check -- `discounted=false` must not be taken for true.
#[test]
fn the_discounted_filter_is_tri_state() {
    let pairs = |v: &str| vec![("discounted".to_string(), v.to_string())];
    assert_eq!(parse_shop_filters(&pairs("true")).discounted, Some(true));
    assert_eq!(parse_shop_filters(&pairs("false")).discounted, Some(false));
    assert_eq!(parse_shop_filters(&pairs("1")).discounted, None);
    assert_eq!(parse_shop_filters(&[]).discounted, None);
}

/// `onSale` stays ignored: the shop already sends it to mean "listed", so honouring it would
/// turn the whole grid into the deals rail.
#[test]
fn on_sale_is_not_the_discounted_filter() {
    let pairs = vec![("onSale".to_string(), "true".to_string())];
    assert_eq!(parse_shop_filters(&pairs).discounted, None);
    let sql = build_shop_listings_sql(&parse_shop_filters(&pairs)).0;
    assert!(!sql.contains("AND cp.id IS NOT NULL"), "{sql}");
}

#[test]
fn the_shop_feed_filters_on_the_coupon_presence() {
    assert!(shop_sql(Some(true)).contains("AND cp.id IS NOT NULL"));
    assert!(shop_sql(Some(false)).contains("AND cp.id IS NULL"));
    let unfiltered = shop_sql(None);
    assert!(
        !unfiltered.contains("AND cp.id IS NOT NULL"),
        "{unfiltered}"
    );
    assert!(!unfiltered.contains("AND cp.id IS NULL"), "{unfiltered}");
}

/// Only the native-primary branch can carry a coupon: the contract mints collection items, so
/// a legacy trade or a CollectionStore mint is never discounted by one.
#[test]
fn only_the_native_branch_of_the_unified_feed_joins_coupons() {
    let sql = unified_sql(None);
    assert_eq!(occurrences(&sql, "FROM marketplace.coupons c"), 1, "{sql}");
    assert_eq!(
        occurrences(&sql, "NULL::jsonb AS coupon"),
        UNION_BRANCHES - 1,
        "the two coupon-free branches still line up for the UNION: {sql}"
    );
    assert_eq!(
        occurrences(&sql, "AS compare_at_usd_wei"),
        UNION_BRANCHES,
        "{sql}"
    );
    assert_eq!(
        occurrences(&sql, "NULL::numeric AS compare_at_usd_wei"),
        UNION_BRANCHES - 1,
        "{sql}"
    );
}

/// A branch that cannot carry a coupon has nothing on sale, so `discounted=true` must empty it
/// rather than leave it unfiltered.
#[test]
fn discounted_true_empties_the_coupon_free_unified_branches() {
    let sql = unified_sql(Some(true));
    assert_eq!(occurrences(&sql, "AND cp.id IS NOT NULL"), 1, "{sql}");
    assert_eq!(occurrences(&sql, "AND FALSE"), UNION_BRANCHES - 1, "{sql}");
}

/// `discounted=false` is a no-op on those branches -- everything in them already qualifies.
#[test]
fn discounted_false_only_constrains_the_coupon_carrying_branch() {
    let sql = unified_sql(Some(false));
    assert_eq!(occurrences(&sql, "AND cp.id IS NULL"), 1, "{sql}");
    assert!(!sql.contains("AND FALSE"), "{sql}");
}

#[test]
fn the_unified_feeds_expose_compare_at_credits() {
    let listings = unified_sql(None);
    assert!(
        listings.contains("sub.compare_at_usd_wei IS NOT NULL"),
        "{listings}"
    );
    assert!(listings.contains("AS compare_at_credits"), "{listings}");

    let (items, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(
        items.contains("f.compare_at_usd_wei IS NOT NULL"),
        "{items}"
    );
    assert!(items.contains("AS compare_at_credits"), "{items}");
}

/// The native branch prices by what the buyer PAYS, so every outer layer -- the credit CEIL,
/// the price bounds and the cheapest sort -- follows without a second expression.
#[test]
fn the_native_branch_prices_by_the_sale_price() {
    let sql = unified_sql(None);
    assert!(
        sql.contains("COALESCE((mv.amount_received::numeric - FLOOR(mv.amount_received::numeric * cp.discount_ppm / 1000000)), mv.amount_received::numeric) AS usd_wei"),
        "{sql}"
    );
}

#[test]
fn the_discount_sort_is_accepted_and_ordered_by_the_coupon() {
    assert_eq!(ShopSortBy::parse("discount"), Some(ShopSortBy::Discount));
    assert!(SHOP_SORT_VALUES.contains(&"discount"));

    let sql = build_unified_listings_sql(
        &UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                sort_by: Some(ShopSortBy::Discount),
                ..Default::default()
            },
            ..Default::default()
        },
        0.5,
    )
    .0;
    assert!(
        sql.contains(
            "ORDER BY sub.coupon_discount_ppm DESC NULLS LAST, sub.sale_ends_at ASC NULLS LAST, sub.trade_id"
        ),
        "{sql}"
    );
}

/// The legacy (MANA) feed carries no coupon join, so `discount` there falls back to newest --
/// upstream's own switch does the same.
#[test]
fn the_legacy_feed_falls_back_to_newest_for_the_discount_sort() {
    let (sql, _) = build_legacy_listings_sql(&LegacyCatalogFilters {
        sort_by: Some(ShopSortBy::Discount),
        ..Default::default()
    });
    assert!(sql.contains("ORDER BY mv.created_at DESC"), "{sql}");
    assert!(!sql.contains("cp.discount_ppm"), "{sql}");
}

const COLLECTION_A: &str = "0x4c09495cd2d4e3d3fa2808eb655d013de426157b";
const COLLECTION_B: &str = "0xb0d0d31910da4a14d4e05a9d51b6e9a99a85d676";

fn coupon_json(collections: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "id": "8f1a6d0e-0000-4000-8000-000000000001",
        "signer": "0x1111111111111111111111111111111111111111",
        "couponManager": "0x3fd3056ee72a2a85e9392fab3a450e7736536081",
        "couponAddress": "0xc914507fe297b2dddd1232ac3a8903f1c125e794",
        "checks": { "uses": 10 },
        "discountType": 1,
        "discount": 300_000,
        "root": "0xfeed",
        "collections": collections,
        "signature": "0xsig",
        "used": 2
    })
}

/// The proof comes from the coupons port's own tree, so the shop and the CouponManager cannot
/// disagree about a root. A one-collection coupon needs no siblings.
#[test]
fn a_catalogue_coupon_carries_the_proof_for_its_own_collection() {
    let one = to_shop_coupon(Some(coupon_json(&[COLLECTION_A])), COLLECTION_A)
        .expect("a covered collection yields a coupon");
    assert!(one.proof.is_empty(), "{:?}", one.proof);
    assert_eq!(one.row.discount, 300_000);
    assert_eq!(one.row.used, 2);

    let two = to_shop_coupon(
        Some(coupon_json(&[COLLECTION_A, COLLECTION_B])),
        COLLECTION_B,
    )
    .expect("a covered collection yields a coupon");
    assert_eq!(two.proof.len(), 1);
    assert!(two.proof[0].starts_with("0x"), "{:?}", two.proof);
}

/// A sale the checkout cannot settle is worse than no sale, so an unprovable or unreadable
/// coupon is dropped rather than shown.
#[test]
fn an_unprovable_coupon_is_dropped_instead_of_shown() {
    assert!(to_shop_coupon(Some(coupon_json(&[COLLECTION_A])), COLLECTION_B).is_none());
    assert!(to_shop_coupon(Some(serde_json::json!({ "id": 3 })), COLLECTION_A).is_none());
    assert!(to_shop_coupon(None, COLLECTION_A).is_none());
}

/// The whole sale block travels together: if the row is not on sale, none of the four fields
/// reaches the wire even when the raw columns were populated.
#[test]
fn the_sale_fields_are_all_null_unless_the_row_is_on_sale() {
    let coupon = to_shop_coupon(Some(coupon_json(&[COLLECTION_A])), COLLECTION_A);
    let off = shop_sale_fields(
        false,
        Some(12),
        Some(1_700_000_000),
        Some(4),
        coupon.clone(),
    );
    assert_eq!(off.compare_at_credits, None);
    assert_eq!(off.sale_ends_at, None);
    assert_eq!(off.sale_units_left, None);
    assert!(off.coupon.is_none());

    let on = shop_sale_fields(true, Some(12), Some(1_700_000_000), Some(4), coupon);
    assert_eq!(on.compare_at_credits, Some(12));
    assert_eq!(on.sale_ends_at, Some(1_700_000_000));
    assert_eq!(on.sale_units_left, Some(4));
    assert!(on.coupon.is_some());
}
