use super::*;

const SOCIAL_EMOTE_EXCLUSION: &str = "AND items.search_emote_outcome_type IS NULL";

fn collections_where_sql(include_social_emotes: Option<bool>) -> String {
    let f = CatalogFilters {
        include_social_emotes,
        ..Default::default()
    };
    let mut b = Builder::new();
    build_collections_where(&mut b, &f, false);
    b.sql
}

fn item_level_where_sql(include_social_emotes: Option<bool>) -> String {
    let f = CatalogFilters {
        include_social_emotes,
        ..Default::default()
    };
    let mut b = Builder::new();
    build_item_level_filters_where(&mut b, &f);
    b.sql
}

#[test]
fn wire_id_is_always_contract_dash_blockchain_id() {
    let row = DbRow {
        id: "0xe1ecb4e5130f493551c7d6df96ad19e5b431a0a9-3lau_hoodie_b_upper_body".into(),
        blockchain_id: "3".into(),
        image: String::new(),
        collection_id: "0xe1ecb4e5130f493551c7d6df96ad19e5b431a0a9".into(),
        rarity: "epic".into(),
        item_type: "wearable_v1".into(),
        price: "0".into(),
        available: "0".into(),
        search_is_store_minter: false,
        search_is_marketplace_v3_minter: false,
        creator: String::new(),
        beneficiary: None,
        created_at: "0".into(),
        updated_at: "0".into(),
        reviewed_at: "0".into(),
        sold_at: "0".into(),
        first_listed_at: None,
        urn: String::new(),
        network: "ETHEREUM".into(),
        metadata: None,
        min_listing_price: None,
        max_listing_price: None,
        open_item_trade_id: None,
        open_item_trade_price: None,
        listings_count: None,
        owners_count: None,
        min_price: None,
        max_price: None,
    };
    let item = from_db_row_to_catalog_item(row, None);
    assert_eq!(item.id, "0xe1ecb4e5130f493551c7d6df96ad19e5b431a0a9-3");
    assert_eq!(item.item_id, "3");
}

#[test]
fn ids_filter_matches_squid_and_canonical_forms() {
    let f = CatalogFilters {
        ids: vec!["0xe1ecb4e5130f493551c7d6df96ad19e5b431a0a9-3".into()],
        ..Default::default()
    };
    let mut b = Builder::new();
    build_collections_where(&mut b, &f, false);
    assert!(b.sql.contains("items.id = ANY($"), "{}", b.sql);
    assert!(
        b.sql
            .contains("(items.collection_id || '-' || items.blockchain_id::text) = ANY($"),
        "canonical ids must keep resolving items whose squid id is the v1 string form: {}",
        b.sql
    );
}

#[test]
fn item_id_filter_applies_to_query_and_count_paths() {
    let f = CatalogFilters {
        item_id: Some("3".to_string()),
        ..Default::default()
    };
    let mut b = Builder::new();
    build_collections_where(&mut b, &f, false);
    assert!(
        b.sql.contains("items.blockchain_id::text = $"),
        "query path must filter by itemId, got: {}",
        b.sql
    );

    let mut b2 = Builder::new();
    build_item_level_filters_where(&mut b2, &f);
    assert!(
        b2.sql.contains("items.blockchain_id::text = $"),
        "count path must filter by itemId, got: {}",
        b2.sql
    );

    let mut b3 = Builder::new();
    build_collections_where(&mut b3, &CatalogFilters::default(), false);
    assert!(!b3.sql.contains("items.blockchain_id::text"));

    let empty = CatalogFilters {
        item_id: Some(String::new()),
        ..Default::default()
    };
    let mut b4 = Builder::new();
    build_collections_where(&mut b4, &empty, false);
    assert!(!b4.sql.contains("items.blockchain_id::text"));
}

#[test]
fn collections_where_excludes_social_emotes_only_when_include_false() {
    let false_sql = collections_where_sql(Some(false));
    assert!(
        false_sql.contains(SOCIAL_EMOTE_EXCLUSION),
        "Some(false) should exclude social emotes, got: {false_sql}"
    );
    assert!(!false_sql.contains("IS NOT NULL"));

    assert!(!collections_where_sql(None).contains(SOCIAL_EMOTE_EXCLUSION));
    assert!(!collections_where_sql(Some(true)).contains(SOCIAL_EMOTE_EXCLUSION));
}

#[test]
fn item_level_filters_where_excludes_social_emotes_only_when_include_false() {
    let false_sql = item_level_where_sql(Some(false));
    assert!(
        false_sql.contains(SOCIAL_EMOTE_EXCLUSION),
        "Some(false) should exclude social emotes, got: {false_sql}"
    );
    assert!(!false_sql.contains("IS NOT NULL"));

    assert!(!item_level_where_sql(None).contains(SOCIAL_EMOTE_EXCLUSION));
    assert!(!item_level_where_sql(Some(true)).contains(SOCIAL_EMOTE_EXCLUSION));
}

#[test]
fn catalog_sorts_prices_numerically_not_lexicographically() {
    for (sort, is_v2) in [
        ("cheapest", false),
        ("cheapest", true),
        ("most_expensive", false),
        ("most_expensive", true),
        ("recently_sold", false),
    ] {
        let f = CatalogFilters {
            sort_by: CatalogSortBy::parse(sort),
            first: Some(24),
            ..Default::default()
        };
        let (sql, _) = if is_v2 {
            build_collections_items_catalog_query_with_trades(&f)
        } else {
            build_collections_items_catalog_query(&f)
        };
        let order_by = sql.rsplit("ORDER BY").next().unwrap_or("");
        assert!(
            !order_by.contains(" min_price asc")
                && !order_by.contains(" max_price desc")
                && !order_by.contains(" sold_at desc")
                || order_by.contains("min_price_num")
                || order_by.contains("max_price_num")
                || order_by.contains("items.sold_at"),
            "{sort} (v2={is_v2}) sorts by a bare ::text alias: ...ORDER BY{order_by}"
        );
        match sort {
            "cheapest" => {
                assert!(order_by.contains("min_price_num asc"), "{order_by}");
                assert!(
                    order_by.contains("items.first_listed_at desc"),
                    "{order_by}"
                );
                assert!(sql.contains("AS min_price_num"), "numeric twin missing");
            }
            "most_expensive" => {
                assert!(order_by.contains("max_price_num desc"), "{order_by}");
                assert!(sql.contains("AS max_price_num"), "numeric twin missing");
            }
            "recently_sold" => {
                assert!(order_by.contains("items.sold_at desc"), "{order_by}");
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn v1_newest_does_not_reference_offchain_orders() {
    let f = CatalogFilters {
        sort_by: CatalogSortBy::parse("newest"),
        first: Some(24),
        ..Default::default()
    };
    let (v1, _) = build_collections_items_catalog_query(&f);
    let v1_order = v1.rsplit("ORDER BY").next().unwrap_or("");
    assert!(!v1_order.contains("offchain_orders"), "{v1_order}");
    assert!(
        v1_order.contains("items.first_listed_at desc nulls LAST"),
        "{v1_order}"
    );
    let (v2, _) = build_collections_items_catalog_query_with_trades(&f);
    assert!(v2
        .rsplit("ORDER BY")
        .next()
        .unwrap_or("")
        .contains("offchain_orders"));
}

#[test]
fn suggested_seen_join_present_wherever_ordered() {
    let suggested = |first| CatalogFilters {
        sort_by: CatalogSortBy::parse("suggested"),
        first,
        ..Default::default()
    };
    const JOIN: &str = "LEFT JOIN marketplace.wearable_last_seen AS seen";
    const ORDER: &str = "COALESCE(EXTRACT(EPOCH FROM seen.last_seen), 0) desc";

    for (label, sql) in [
        (
            "v1 paged",
            build_collections_items_catalog_query(&suggested(Some(24))).0,
        ),
        (
            "v1 unpaged",
            build_collections_items_catalog_query(&suggested(None)).0,
        ),
        (
            "v2",
            build_collections_items_catalog_query_with_trades(&suggested(Some(24))).0,
        ),
    ] {
        assert_eq!(
            sql.matches(JOIN).count(),
            sql.matches(ORDER).count(),
            "{label}: every ORDER BY on seen needs its own join in scope\n{sql}"
        );
        assert!(
            sql.contains(ORDER),
            "{label}: suggested ORDER BY missing\n{sql}"
        );
        assert!(
            sql.contains("items.first_listed_at desc nulls LAST"),
            "{label}: never-seen fallback ordering missing\n{sql}"
        );
    }

    let other = CatalogFilters {
        sort_by: CatalogSortBy::parse("newest"),
        first: Some(24),
        ..Default::default()
    };
    assert!(!build_collections_items_catalog_query(&other)
        .0
        .contains(JOIN));
    assert!(!build_collections_items_catalog_query_with_trades(&other)
        .0
        .contains(JOIN));
}

#[test]
fn v1_two_pass_shape() {
    let paged = CatalogFilters {
        first: Some(24),
        ..Default::default()
    };
    let (sql, _) = build_collections_items_catalog_query(&paged);
    assert!(
        sql.contains("WITH nfts_with_orders AS MATERIALIZED"),
        "aggregate must be a shared CTE"
    );
    assert!(sql.contains("ranked AS"), "ranking pass missing");
    assert!(
        sql.contains("JOIN ranked ON ranked.ranked_id = items.id"),
        "payload pass must join the page"
    );
    assert_eq!(sql.matches("LIMIT $").count(), 1, "{sql}");

    let unpaged = CatalogFilters::default();
    let (sql, _) = build_collections_items_catalog_query(&unpaged);
    assert!(
        !sql.contains("ranked AS"),
        "unpaged queries stay single-pass"
    );

    let hat = CatalogFilters {
        first: Some(24),
        wearable_category: Some("hat".into()),
        ..Default::default()
    };
    let (sql, _) = build_collections_items_catalog_query(&hat);
    let ranked_part = sql
        .split("ranked AS")
        .nth(1)
        .unwrap()
        .split(") ")
        .next()
        .unwrap_or("");
    let _ = ranked_part;
    assert!(sql.contains("ranked AS"), "{sql}");
    assert_eq!(
        sql.matches("AS metadata_wearable").count(),
        2,
        "hat filter must join metadata in BOTH passes"
    );
}
