use super::*;

const NO_ITEM_ARM: &str = "COALESCE(item_p.id, item_s.id) IS NULL";
const APPROVED_ARM: &str =
    "OR COALESCE(item_p.search_is_collection_approved, item_s.search_is_collection_approved) = true";

/// A LAND, estate or name row has no item and survives on the first arm; an item whose flag
/// is NULL fails the second; one COALESCE with a default could not tell those apart.
fn assert_two_armed_approval(sql: &str) {
    assert!(
        sql.contains(&format!("(\n{NO_ITEM_ARM}\n{APPROVED_ARM}\n)")),
        "{sql}"
    );
}

/// Upstream dfc17f9 appends the predicate unconditionally.
#[test]
fn every_trade_feed_hides_unapproved_collections_without_any_filter() {
    let (shop, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert_two_armed_approval(&shop);
    assert_eq!(shop.matches(APPROVED_ARM).count(), 1, "{shop}");

    let (legacy, _) = build_legacy_listings_sql(&LegacyCatalogFilters::default());
    assert_two_armed_approval(&legacy);
    assert_eq!(legacy.matches(APPROVED_ARM).count(), 1, "{legacy}");

    let (unified, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_two_armed_approval(&unified);
    assert_eq!(
        unified.matches(APPROVED_ARM).count(),
        unified.matches("UNION ALL").count() + 1,
        "one predicate per unified branch, the store branch included: {unified}"
    );
}

#[test]
fn the_approval_predicate_survives_every_browse_filter() {
    let filters = ShopCatalogFilters {
        search: Some("hat".to_string()),
        category: Some("wearable".to_string()),
        contract_address: Some("0xABC".to_string()),
        ..Default::default()
    };
    let (shop, _) = build_shop_listings_sql(&filters);
    assert_two_armed_approval(&shop);
    let (unified, _) = build_unified_listings_sql(
        &UnifiedCatalogFilters {
            base: filters,
            ..Default::default()
        },
        0.5,
    );
    assert_two_armed_approval(&unified);
}

/// Upstream left this path alone: a seller migrating their own listings sees all of them.
#[test]
fn the_importable_feed_does_not_gain_the_approval_predicate() {
    let (sql, _) = build_importable_listings_sql("0xabc");
    assert!(!sql.contains("search_is_collection_approved"), "{sql}");
}
