use super::*;

#[test]
fn shop_search_escapes_ilike_wildcards() {
    let filters = ShopCatalogFilters {
        search: Some("50%_off".to_string()),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(
        sql.contains("(COALESCE(item_p.id, item_s.id) IS NULL AND nft.name ILIKE $"),
        "{sql}"
    );
    assert!(bind_texts(&binds).contains(&"%50\\%\\_off%".to_string()));
    assert!(bind_texts(&binds).contains(&"50%_off".to_string()));
}

/// Upstream dfc17f9: only rows that are not collection items (LAND, estates, names) fall back
/// to a name substring.
#[test]
fn shop_search_matches_item_words_and_collection_words() {
    let filters = ShopCatalogFilters {
        search: Some("pirate hat".to_string()),
        ..Default::default()
    };
    let (sql, _) = build_shop_listings_sql(&filters);
    assert!(
        sql.contains(
            "unnest(string_to_array(COALESCE(nft.name, w_p.name, e_p.name), ' ')) AS search_word(text)"
        ),
        "{sql}"
    );
    assert!(sql.contains("lower(search_word.text) % lower($"), "{sql}");
    assert!(
        sql.contains("search_collection.id = COALESCE(item_p.collection_id, item_s.collection_id)"),
        "{sql}"
    );
    assert!(
        !sql.contains("COALESCE(nft.name, w_p.name, e_p.name) ILIKE"),
        "the name substring is the fallback for non-item rows only:\n{sql}"
    );
    assert!(
        sql.contains("((COALESCE(item_p.id, item_s.id) IS NOT NULL AND (EXISTS (SELECT 1 FROM unnest("),
        "the word arms fire only for rows with an item; a LAND row matches through the substring alone:\n{sql}"
    );
    let (unified, _) = build_unified_listings_sql(
        &UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                search: Some("pirate hat".to_string()),
                ..Default::default()
            },
            ..Default::default()
        },
        0.5,
    );
    assert!(
        unified.contains("lower(search_word.text) % lower($"),
        "{unified}"
    );
    assert!(
        unified.contains("(COALESCE(item_p.id, item_s.id) IS NULL AND nft.name ILIKE $"),
        "{unified}"
    );
    assert!(
        unified.contains(
            "((COALESCE(item_p.id, item_s.id) IS NOT NULL AND (EXISTS (SELECT 1 FROM unnest("
        ),
        "{unified}"
    );
}
