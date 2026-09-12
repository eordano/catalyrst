use super::*;

/// Upstream 587defd. `NULL` (not an emote) and `false` (an emote that plays once) are
/// different answers, so the column travels raw and is never defaulted.
#[test]
fn unified_selects_the_emote_play_mode_from_either_side_of_the_join() {
    const COLUMN: &str =
        "COALESCE(item_p.search_emote_loop, item_s.search_emote_loop) AS emote_loop";

    let (listings, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(occurrences(&listings, COLUMN), 3, "{listings}");

    let (items, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(occurrences(&items, COLUMN), 3, "{items}");

    let (related, _) = build_related_items_sql(
        "0xcollection",
        "3",
        &ReferenceItem {
            category: "emote",
            wearable_category: Some("dance".to_string()),
            rarity: Some("rare".to_string()),
        },
        None,
        0.5,
    );
    assert_eq!(occurrences(&related, COLUMN), 3, "{related}");

    let (trending, _) =
        build_trending_items_sql(None, None, &UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(occurrences(&trending, COLUMN), 3, "{trending}");
}

/// Upstream left the legacy per-listing feed alone.
#[test]
fn the_legacy_shop_feed_does_not_gain_the_emote_play_mode() {
    let (sql, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert!(!sql.contains("emote_loop"), "{sql}");
}
