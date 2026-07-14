use super::component::{network_and_chain, top_level_category};
use super::sql::{
    build_importable_listings_sql, build_legacy_listings_sql, build_shop_listings_sql,
    build_top_creators_sql, credits_to_wei, escape_like, to_credits, Bind, ASSET_TYPE_ERC20,
    ASSET_TYPE_USD_PEGGED_MANA, USD_WEI_PER_CREDIT,
};
use super::types::{
    parse_shop_filters, LegacyCatalogFilters, ShopCatalogFilters, ShopSortBy,
    TOP_CREATORS_MAX_DAYS, TOP_CREATORS_MAX_LIMIT, TOP_CREATORS_MIN_DAYS, TOP_CREATORS_MIN_ITEMS,
    TOP_CREATORS_MIN_LIMIT, TOP_CREATORS_MIN_SALES_PER_WINDOW, TOP_CREATORS_MIN_WINDOW_SALES_FLOOR,
};
use super::unified::{
    build_reference_item_sql, build_related_items_sql, build_trending_items_sql,
    build_unified_items_sql, build_unified_listings_sql, parse_trending_filters,
    parse_unified_filters, parse_unified_group_by, unified_min_price_bound_wei, ReferenceItem,
    ShopListingType, UnifiedCatalogFilters, UnifiedGroupBy, UnifiedSource, RELATED_DEFAULT_LIMIT,
    RELATED_MAX_LIMIT,
};
use crate::dcl_schemas::Network;

fn bind_texts(binds: &[Bind]) -> Vec<String> {
    binds
        .iter()
        .filter_map(|b| match b {
            Bind::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn bind_ints(binds: &[Bind]) -> Vec<i64> {
    binds
        .iter()
        .filter_map(|b| match b {
            Bind::Int(i) => Some(*i),
            _ => None,
        })
        .collect()
}

fn bind_arrays(binds: &[Bind]) -> Vec<Vec<String>> {
    binds
        .iter()
        .filter_map(|b| match b {
            Bind::TextArray(v) => Some(v.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn shop_sql_targets_open_credit_buyable_listings() {
    let (sql, binds) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert!(sql.contains("mv.status = 'open'"), "{sql}");
    assert!(
        sql.contains("mv.available IS NULL OR mv.available > 0"),
        "{sql}"
    );
    assert!(
        sql.contains("ta.direction = 'received' AND ta.asset_type = $1"),
        "{sql}"
    );
    assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
    assert!(sql.contains("marketplace.mv_trades mv"), "{sql}");
    assert!(
        sql.contains("item_p.blockchain_id = mv.sent_item_id::numeric"),
        "{sql}"
    );
    assert!(
        sql.contains("nft ON mv.type = 'public_nft_order' AND nft.id = mv.sent_nft_id"),
        "{sql}"
    );
    assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_USD_PEGGED_MANA, 48, 0]);
}

#[test]
fn shop_price_bounds_bind_whole_credit_wei() {
    let filters = ShopCatalogFilters {
        min_price_credits: Some(3.0),
        max_price_credits: Some(10.0),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(sql.contains("mv.amount_received >= $"), "{sql}");
    assert!(sql.contains("mv.amount_received <= $"), "{sql}");
    let texts = bind_texts(&binds);
    assert!(texts.contains(&(3 * USD_WEI_PER_CREDIT).to_string()));
    assert!(texts.contains(&(10 * USD_WEI_PER_CREDIT).to_string()));
}

#[test]
fn shop_non_finite_price_bounds_are_skipped() {
    let filters = ShopCatalogFilters {
        min_price_credits: Some(f64::INFINITY),
        max_price_credits: Some(f64::NAN),
        ..Default::default()
    };
    let (sql, _) = build_shop_listings_sql(&filters);
    assert!(!sql.contains("mv.amount_received >="), "{sql}");
    assert!(!sql.contains("mv.amount_received <="), "{sql}");
}

#[test]
fn shop_search_escapes_ilike_wildcards() {
    let filters = ShopCatalogFilters {
        search: Some("50%_off".to_string()),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(
        sql.contains("COALESCE(nft.name, w_p.name, e_p.name) ILIKE $"),
        "{sql}"
    );
    assert!(bind_texts(&binds).contains(&"%50\\%\\_off%".to_string()));
}

#[test]
fn shop_sort_uses_fixed_expressions_only() {
    for (sort, expected) in [
        (
            Some(ShopSortBy::Cheapest),
            "ORDER BY mv.amount_received ASC",
        ),
        (
            Some(ShopSortBy::MostExpensive),
            "ORDER BY mv.amount_received DESC",
        ),
        (
            Some(ShopSortBy::Name),
            "ORDER BY COALESCE(nft.name, w_p.name, e_p.name) ASC",
        ),
        (Some(ShopSortBy::Newest), "ORDER BY mv.created_at DESC"),
        (None, "ORDER BY mv.created_at DESC"),
    ] {
        let filters = ShopCatalogFilters {
            sort_by: sort,
            ..Default::default()
        };
        let (sql, _) = build_shop_listings_sql(&filters);
        assert!(sql.contains(expected), "{sort:?}: {sql}");
    }
}

#[test]
fn shop_pagination_is_clamped() {
    let filters = ShopCatalogFilters {
        first: Some(99_999),
        skip: Some(-5),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(sql.contains("LIMIT $"), "{sql}");
    assert!(sql.contains("OFFSET $"), "{sql}");
    let ints = bind_ints(&binds);
    assert!(ints.contains(&super::types::SHOP_MAX_PAGE_SIZE));
    assert!(ints.contains(&0));

    let (_, binds) = build_shop_listings_sql(&ShopCatalogFilters {
        first: Some(0),
        ..Default::default()
    });
    assert!(bind_ints(&binds).contains(&super::types::SHOP_MIN_PAGE_SIZE));
}

#[test]
fn shop_rarities_and_categories_are_lowercased_array_binds() {
    let filters = ShopCatalogFilters {
        rarities: vec!["Rare".to_string(), "EPIC".to_string()],
        wearable_categories: vec!["Upper_Body".to_string(), "HAT".to_string()],
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(
        sql.contains(
            "lower(COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity)) = ANY($"
        ),
        "{sql}"
    );
    assert!(
        sql.contains(
            "lower(COALESCE(item_p.search_wearable_category, item_s.search_wearable_category"
        ),
        "{sql}"
    );
    let arrays = bind_arrays(&binds);
    assert!(arrays.contains(&vec!["rare".to_string(), "epic".to_string()]));
    assert!(arrays.contains(&vec!["upper_body".to_string(), "hat".to_string()]));
}

#[test]
fn shop_contract_address_is_lowercased() {
    let filters = ShopCatalogFilters {
        contract_address: Some("0xABCdef".to_string()),
        item_id: Some("3".to_string()),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(sql.contains("mv.sent_contract_address = $"), "{sql}");
    assert!(sql.contains("mv.sent_item_id = $"), "{sql}");
    let texts = bind_texts(&binds);
    assert!(texts.contains(&"0xabcdef".to_string()));
    assert!(texts.contains(&"3".to_string()));
}

#[test]
fn importable_sql_is_seller_scoped_classic_mana_and_capped() {
    let (sql, binds) = build_importable_listings_sql("0xABCdef");
    assert!(sql.contains("lower(mv.signer) = $1"), "{sql}");
    assert!(
        sql.contains("ta.direction = 'received' AND ta.asset_type = $2"),
        "{sql}"
    );
    assert!(sql.contains("ORDER BY mv.created_at DESC"), "{sql}");
    assert!(sql.contains("LIMIT $3"), "{sql}");
    assert!(
        sql.contains("mv.amount_received::text AS mana_wei"),
        "{sql}"
    );
    assert!(bind_texts(&binds).contains(&"0xabcdef".to_string()));
    assert_eq!(
        bind_ints(&binds),
        vec![ASSET_TYPE_ERC20, super::types::SHOP_MAX_PAGE_SIZE]
    );
}

#[test]
fn legacy_sql_is_primary_only_classic_mana() {
    let (sql, binds) = build_legacy_listings_sql(&LegacyCatalogFilters::default());
    assert!(sql.contains("mv.status = 'open'"), "{sql}");
    assert!(sql.contains("mv.type = 'public_item_order'"), "{sql}");
    assert!(
        sql.contains("mv.available IS NULL OR mv.available > 0"),
        "{sql}"
    );
    assert!(
        sql.contains("ta.direction = 'received' AND ta.asset_type = $1"),
        "{sql}"
    );
    assert!(
        sql.contains("mv.amount_received::text AS mana_wei"),
        "{sql}"
    );
    assert!(!sql.contains("mv.amount_received >="), "{sql}");
    assert!(!sql.contains("mv.amount_received <="), "{sql}");
    assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
    assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_ERC20, 48, 0]);
}

#[test]
fn legacy_filters_use_primary_columns_only() {
    let filters = LegacyCatalogFilters {
        rarities: vec!["Rare".to_string()],
        wearable_categories: vec!["HAT".to_string()],
        search: Some("50%_off".to_string()),
        sort_by: Some(ShopSortBy::Name),
        ..Default::default()
    };
    let (sql, binds) = build_legacy_listings_sql(&filters);
    assert!(sql.contains("lower(item_p.rarity) = ANY($"), "{sql}");
    assert!(
        sql.contains(
            "lower(COALESCE(item_p.search_wearable_category, item_p.search_emote_category)) = ANY($"
        ),
        "{sql}"
    );
    assert!(
        sql.contains("COALESCE(w_p.name, e_p.name) ILIKE $"),
        "{sql}"
    );
    assert!(
        sql.contains("ORDER BY COALESCE(w_p.name, e_p.name) ASC"),
        "{sql}"
    );
    assert!(bind_texts(&binds).contains(&"%50\\%\\_off%".to_string()));
    let arrays = bind_arrays(&binds);
    assert!(arrays.contains(&vec!["rare".to_string()]));
    assert!(arrays.contains(&vec!["hat".to_string()]));
}

#[test]
fn to_credits_ceils_and_drops_bad_amounts() {
    assert_eq!(to_credits(&USD_WEI_PER_CREDIT.to_string()), Some(1));
    assert_eq!(to_credits(&(5 * USD_WEI_PER_CREDIT).to_string()), Some(5));
    assert_eq!(
        to_credits(&(USD_WEI_PER_CREDIT + 1).to_string()),
        Some(2),
        "non-conforming price rounds up, never advertised below settlement"
    );
    assert_eq!(to_credits("1"), Some(1));
    assert_eq!(to_credits("0"), None);
    assert_eq!(to_credits("-5"), None);
    assert_eq!(to_credits("not-a-number"), None);
    assert_eq!(to_credits(""), None);
}

#[test]
fn credits_to_wei_floors_and_clamps() {
    assert_eq!(credits_to_wei(3.7), Some(3 * USD_WEI_PER_CREDIT));
    assert_eq!(credits_to_wei(-5.0), Some(0));
    assert_eq!(credits_to_wei(f64::INFINITY), None);
    assert_eq!(credits_to_wei(f64::NAN), None);
}

#[test]
fn escape_like_neutralizes_metacharacters() {
    assert_eq!(escape_like("50%_off"), "50\\%\\_off");
    assert_eq!(escape_like("a\\b"), "a\\\\b");
    assert_eq!(escape_like("plain"), "plain");
}

#[test]
fn top_level_category_splits_on_emote_prefix() {
    assert_eq!(top_level_category(Some("emote_v1")), "emote");
    assert_eq!(top_level_category(Some("EMOTE_V1")), "emote");
    assert_eq!(top_level_category(Some("wearable_v2")), "wearable");
    assert_eq!(top_level_category(None), "wearable");
}

#[test]
fn network_defaults_to_matic() {
    assert_eq!(network_and_chain(None).0, Network::Matic);
    assert_eq!(network_and_chain(Some("POLYGON")).0, Network::Matic);
    assert_eq!(network_and_chain(Some("ETHEREUM")).0, Network::Ethereum);
    assert_eq!(network_and_chain(Some("ethereum")).0, Network::Ethereum);
}

#[test]
fn parse_shop_filters_validates_sort_and_splits_csv() {
    let pairs = vec![
        ("first".to_string(), "10".to_string()),
        ("skip".to_string(), "Infinity".to_string()),
        ("rarity".to_string(), "rare, epic,".to_string()),
        ("wearableCategory".to_string(), "hat".to_string()),
        ("sortBy".to_string(), "cheapest".to_string()),
    ];
    let f = parse_shop_filters(&pairs);
    assert_eq!(f.first, Some(10));
    assert_eq!(f.skip, None);
    assert_eq!(f.rarities, vec!["rare".to_string(), "epic".to_string()]);
    assert_eq!(f.wearable_categories, vec!["hat".to_string()]);
    assert_eq!(f.sort_by, Some(ShopSortBy::Cheapest));

    let bad = vec![("sortBy".to_string(), "1; DROP TABLE".to_string())];
    assert_eq!(parse_shop_filters(&bad).sort_by, None);
}

const GENDER_UNISEX_ARM: &str =
    "COALESCE(item_p.search_wearable_body_shapes, item_s.search_wearable_body_shapes)::text[] \
     @> ARRAY['BaseMale','BaseFemale']::text[] THEN 'unisex'";

#[test]
fn shop_and_legacy_feeds_expose_body_shape_derived_gender() {
    let (shop_sql, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    let (legacy_sql, _) = build_legacy_listings_sql(&LegacyCatalogFilters::default());
    for sql in [&shop_sql, &legacy_sql] {
        assert!(sql.contains(GENDER_UNISEX_ARM), "{sql}");
        assert!(sql.contains("THEN 'male'"), "{sql}");
        assert!(sql.contains("THEN 'female'"), "{sql}");
        assert!(sql.contains("END AS gender"), "{sql}");
    }
    let (importable_sql, _) = build_importable_listings_sql("0xabc");
    assert!(!importable_sql.contains("AS gender"), "{importable_sql}");
}

#[test]
fn gender_column_stays_separated_from_the_from_clause() {
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(!sql.contains("genderFROM"), "{sql}");
    assert!(
        sql.contains("END AS gender\nFROM marketplace.mv_trades"),
        "{sql}"
    );
}

#[test]
fn unified_defaults_to_both_sources_merged_with_union_all() {
    let (sql, binds) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(
        occurrences(&sql, "UNION ALL"),
        2,
        "native trade + legacy trade + store: {sql}"
    );
    assert!(sql.contains("'native' AS source"), "{sql}");
    assert!(sql.contains("'legacy' AS source"), "{sql}");
    assert_eq!(occurrences(&sql, "'trade' AS acquisition"), 2, "{sql}");
    assert_eq!(occurrences(&sql, "'store' AS acquisition"), 1, "{sql}");
    assert!(
        sql.contains("mv.amount_received::numeric AS usd_wei"),
        "{sql}"
    );
    assert!(
        sql.contains("(mv.amount_received::numeric * $1::numeric) AS usd_wei"),
        "{sql}"
    );
    assert!(sql.contains("NULL::text AS mana_wei"), "{sql}");
    assert!(
        sql.contains("mv.amount_received::text AS mana_wei"),
        "{sql}"
    );
    assert!(
        sql.contains("CEIL(sub.usd_wei / 100000000000000000::numeric)::bigint AS price_credits"),
        "{sql}"
    );
    assert!(sql.contains("WHERE sub.usd_wei > 0"), "{sql}");
    assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
    assert_eq!(bind_texts(&binds)[0], "0.500000000000000000");
    assert_eq!(
        bind_ints(&binds),
        vec![ASSET_TYPE_USD_PEGGED_MANA, ASSET_TYPE_ERC20, 48, 0]
    );
}

#[test]
fn unified_legacy_branch_is_primary_only_but_native_keeps_secondaries() {
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    let native = sql.split("UNION ALL").next().unwrap();
    let legacy = sql.split("UNION ALL").nth(1).unwrap();
    let primary_only = "AND mv.type = 'public_item_order' AND EXISTS";
    assert!(!native.contains(primary_only), "{native}");
    assert!(legacy.contains(primary_only), "{legacy}");
}

#[test]
fn unified_source_filter_restricts_branches_native_being_the_only_single_branch_one() {
    let native_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Native),
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&native_only, 0.5);
    assert!(!sql.contains("UNION ALL"), "{sql}");
    assert!(sql.contains("'native' AS source"), "{sql}");
    assert!(!sql.contains("'legacy' AS source"), "{sql}");
    assert!(
        !sql.contains("'store' AS acquisition"),
        "store rows are legacy-priced, so source=native excludes them: {sql}"
    );
    assert!(
        bind_texts(&binds).is_empty(),
        "no rate bind for native-only"
    );

    // legacy is now TWO branches: the offchain primary trade + the
    // CollectionStore mint, both MANA-priced through the same rate bind.
    let legacy_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Legacy),
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&legacy_only, 0.5);
    assert_eq!(occurrences(&sql, "UNION ALL"), 1, "{sql}");
    assert!(!sql.contains("'native' AS source"), "{sql}");
    assert_eq!(occurrences(&sql, "'legacy' AS source"), 2, "{sql}");
    assert_eq!(occurrences(&sql, "'store' AS acquisition"), 1, "{sql}");
    assert_eq!(bind_texts(&binds)[0], "0.500000000000000000");
}

#[test]
fn unified_min_credit_filter_is_ceil_consistent() {
    let bound = unified_min_price_bound_wei(3.0).unwrap();
    assert_eq!(bound, 2 * USD_WEI_PER_CREDIT);
    let displays_as_three = 2 * USD_WEI_PER_CREDIT + 1;
    let displays_as_two = 2 * USD_WEI_PER_CREDIT;
    assert!(displays_as_three > bound);
    assert!(displays_as_two <= bound);

    assert_eq!(unified_min_price_bound_wei(1.0), Some(0));
    assert_eq!(unified_min_price_bound_wei(0.0), None);
    assert_eq!(unified_min_price_bound_wei(-2.0), None);
    assert_eq!(unified_min_price_bound_wei(f64::INFINITY), None);
    assert_eq!(unified_min_price_bound_wei(f64::NAN), None);
}

#[test]
fn unified_price_bounds_apply_to_the_merged_set() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            min_price_credits: Some(3.0),
            max_price_credits: Some(10.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&filters, 0.5);
    assert!(sql.contains("sub.usd_wei > $"), "{sql}");
    assert!(sql.contains("sub.usd_wei <= $"), "{sql}");
    let texts = bind_texts(&binds);
    assert!(texts.contains(&(2 * USD_WEI_PER_CREDIT).to_string()));
    assert!(texts.contains(&(10 * USD_WEI_PER_CREDIT).to_string()));

    let no_min = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            min_price_credits: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, _) = build_unified_listings_sql(&no_min, 0.5);
    assert!(!sql.contains("sub.usd_wei > $"), "{sql}");
}

#[test]
fn unified_sort_is_total_ordered_on_the_merged_set() {
    for (sort, expected) in [
        (
            Some(ShopSortBy::Cheapest),
            "ORDER BY sub.usd_wei ASC, sub.trade_id",
        ),
        (
            Some(ShopSortBy::MostExpensive),
            "ORDER BY sub.usd_wei DESC, sub.trade_id",
        ),
        (
            Some(ShopSortBy::Name),
            "ORDER BY sub.name ASC, sub.trade_id",
        ),
        (
            Some(ShopSortBy::Newest),
            "ORDER BY sub.created_at DESC, sub.trade_id",
        ),
        (None, "ORDER BY sub.created_at DESC, sub.trade_id"),
    ] {
        let filters = UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                sort_by: sort,
                ..Default::default()
            },
            ..Default::default()
        };
        let (sql, _) = build_unified_listings_sql(&filters, 0.5);
        assert!(sql.contains(expected), "{sort:?}: {sql}");
    }
}

#[test]
fn unified_browse_filters_apply_inside_each_branch() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            contract_address: Some("0xABC".to_string()),
            category: Some("emote".to_string()),
            search: Some("dance".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&filters, 0.5);
    assert_eq!(
        sql.matches("mv.sent_contract_address = $").count(),
        3,
        "{sql}"
    );
    assert_eq!(
        sql.matches("COALESCE(item_p.item_type, item_s.item_type, nft.item_type) ILIKE 'emote%'")
            .count(),
        3,
        "{sql}"
    );
    let texts = bind_texts(&binds);
    assert_eq!(
        texts.iter().filter(|t| *t == "0xabc").count(),
        3,
        "{texts:?}"
    );
    assert_eq!(
        texts.iter().filter(|t| *t == "%dance%").count(),
        3,
        "{texts:?}"
    );
}

#[test]
fn unified_broken_rate_binds_zero_so_legacy_rows_drop() {
    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let (_, binds) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), rate);
        assert_eq!(bind_texts(&binds)[0], "0", "rate {rate}");
    }
}

const CREATOR_PREDICATE: &str = "lower(COALESCE(item_p.creator, item_s.creator, '')) = $";

#[test]
fn shop_creator_filter_binds_a_lowercased_address() {
    let filters = ShopCatalogFilters {
        creator: Some("0xCREATOR".to_string()),
        ..Default::default()
    };
    let (sql, binds) = build_shop_listings_sql(&filters);
    assert!(sql.contains(CREATOR_PREDICATE), "{sql}");
    assert!(bind_texts(&binds).contains(&"0xcreator".to_string()));

    let (sql, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert!(!sql.contains(CREATOR_PREDICATE), "{sql}");

    let (sql, _) = build_shop_listings_sql(&ShopCatalogFilters {
        creator: Some(String::new()),
        ..Default::default()
    });
    assert!(!sql.contains(CREATOR_PREDICATE), "{sql}");
}

#[test]
fn unified_creator_filter_applies_inside_each_branch() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            creator: Some("0xCREATOR".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&filters, 0.5);
    assert_eq!(sql.matches(CREATOR_PREDICATE).count(), 3, "{sql}");
    assert_eq!(
        bind_texts(&binds)
            .iter()
            .filter(|t| *t == "0xcreator")
            .count(),
        3
    );

    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(!sql.contains(CREATOR_PREDICATE), "{sql}");
}

const SMART_WEARABLE_PREDICATE: &str =
    "COALESCE(item_p.item_type, item_s.item_type, nft.item_type) = 'smart_wearable_v1'";

#[test]
fn shop_smart_wearable_filter_is_presence_gated() {
    let filters = ShopCatalogFilters {
        is_smart: true,
        ..Default::default()
    };
    let (sql, _) = build_shop_listings_sql(&filters);
    assert!(sql.contains(SMART_WEARABLE_PREDICATE), "{sql}");

    let (sql, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert!(!sql.contains("= 'smart_wearable_v1'"), "{sql}");
}

#[test]
fn unified_smart_wearable_filter_applies_inside_each_branch() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            is_smart: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, _) = build_unified_listings_sql(&filters, 0.5);
    assert_eq!(sql.matches(SMART_WEARABLE_PREDICATE).count(), 3, "{sql}");

    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(!sql.contains("= 'smart_wearable_v1'"), "{sql}");
}

#[test]
fn parse_shop_filters_reads_creator_and_is_smart() {
    let pairs = vec![
        ("creator".to_string(), "0xAbC".to_string()),
        ("isSmart".to_string(), "true".to_string()),
    ];
    let f = parse_shop_filters(&pairs);
    assert_eq!(f.creator.as_deref(), Some("0xAbC"));
    assert!(f.is_smart);

    let f = parse_shop_filters(&[]);
    assert_eq!(f.creator, None);
    assert!(!f.is_smart);
}

const SELLER_EXPR: &str = "mv.assets->'sent'->>'owner' AS seller";
const ISSUED_ID_EXPR: &str = "mv.assets->'sent'->>'issued_id' AS issued_id";

#[test]
fn shop_feed_selects_seller_and_issued_id_from_the_sent_asset_json() {
    let (sql, _) = build_shop_listings_sql(&ShopCatalogFilters::default());
    assert!(sql.contains(SELLER_EXPR), "{sql}");
    assert!(sql.contains(ISSUED_ID_EXPR), "{sql}");
}

#[test]
fn unified_feeds_select_seller_and_issued_id_in_each_branch() {
    // Three branches now: the store branch reads the same expressions off its
    // NULL::jsonb assets projection, so seller/issued_id land as NULL rows.
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(sql.matches(SELLER_EXPR).count(), 3, "{sql}");
    assert_eq!(sql.matches(ISSUED_ID_EXPR).count(), 3, "{sql}");

    let (sql, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(sql.matches(SELLER_EXPR).count(), 3, "{sql}");
    assert_eq!(sql.matches(ISSUED_ID_EXPR).count(), 3, "{sql}");
}

#[test]
fn unified_items_collapse_to_one_row_per_item() {
    let (sql, binds) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(
        sql.contains("SELECT DISTINCT ON (f.contract_address, f.item_id)"),
        "{sql}"
    );
    assert!(
        sql.contains("COUNT(*) OVER (PARTITION BY u.contract_address, u.item_id) AS listing_count"),
        "{sql}"
    );
    assert!(sql.contains("WHERE u.usd_wei > 0"), "{sql}");
    assert!(
        sql.contains("(CASE WHEN f.trade_type = 'public_item_order' THEN 0 ELSE 1 END)"),
        "{sql}"
    );
    assert!(
        sql.contains("(CASE WHEN f.source = 'native' THEN 0 ELSE 1 END)"),
        "{sql}"
    );
    assert!(sql.contains("f.usd_wei ASC"), "{sql}");
    assert!(sql.contains("f.trade_id"), "{sql}");
    assert!(
        sql.contains("CEIL(f.usd_wei / 100000000000000000::numeric)::bigint AS price_credits"),
        "{sql}"
    );
    assert!(sql.contains("COUNT(*) OVER() AS total"), "{sql}");
    assert!(sql.contains("UNION ALL"), "{sql}");
    assert_eq!(bind_texts(&binds)[0], "0.500000000000000000");
    assert_eq!(
        bind_ints(&binds),
        vec![ASSET_TYPE_USD_PEGGED_MANA, ASSET_TYPE_ERC20, 48, 0]
    );
}

#[test]
fn unified_items_source_filter_restricts_branches() {
    let native_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Native),
        ..Default::default()
    };
    let (sql, binds) = build_unified_items_sql(&native_only, 0.5);
    assert!(!sql.contains("UNION ALL"), "{sql}");
    assert!(sql.contains("'native' AS source"), "{sql}");
    assert!(!sql.contains("'legacy' AS source"), "{sql}");
    assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_USD_PEGGED_MANA, 48, 0]);

    // legacy = the offchain primary trade branch + the store branch; only the
    // trade branch carries the received-asset EXISTS bind.
    let legacy_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Legacy),
        ..Default::default()
    };
    let (sql, binds) = build_unified_items_sql(&legacy_only, 0.5);
    assert_eq!(occurrences(&sql, "UNION ALL"), 1, "{sql}");
    assert_eq!(occurrences(&sql, "'legacy' AS source"), 2, "{sql}");
    assert_eq!(occurrences(&sql, "'store' AS acquisition"), 1, "{sql}");
    assert_eq!(bind_ints(&binds), vec![ASSET_TYPE_ERC20, 48, 0]);
}

#[test]
fn unified_items_price_bounds_apply_to_the_headline_price() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            min_price_credits: Some(3.0),
            max_price_credits: Some(10.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, binds) = build_unified_items_sql(&filters, 0.5);
    assert!(sql.contains("d.usd_wei > $"), "{sql}");
    assert!(sql.contains("d.usd_wei <= $"), "{sql}");
    let texts = bind_texts(&binds);
    assert!(texts.contains(&(2 * USD_WEI_PER_CREDIT).to_string()));
    assert!(texts.contains(&(10 * USD_WEI_PER_CREDIT).to_string()));

    let no_min = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            min_price_credits: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, _) = build_unified_items_sql(&no_min, 0.5);
    assert!(!sql.contains("d.usd_wei > $"), "{sql}");
}

#[test]
fn unified_items_sort_on_the_deduped_set_with_a_stable_tiebreaker() {
    for (sort, expected) in [
        (
            Some(ShopSortBy::Cheapest),
            "ORDER BY d.usd_wei ASC, d.trade_id",
        ),
        (
            Some(ShopSortBy::MostExpensive),
            "ORDER BY d.usd_wei DESC, d.trade_id",
        ),
        (Some(ShopSortBy::Name), "ORDER BY d.name ASC, d.trade_id"),
        (
            Some(ShopSortBy::Newest),
            "ORDER BY d.created_at DESC, d.trade_id",
        ),
        (None, "ORDER BY d.created_at DESC, d.trade_id"),
    ] {
        let filters = UnifiedCatalogFilters {
            base: ShopCatalogFilters {
                sort_by: sort,
                ..Default::default()
            },
            ..Default::default()
        };
        let (sql, _) = build_unified_items_sql(&filters, 0.5);
        assert!(sql.contains(expected), "{sort:?}: {sql}");
    }
}

#[test]
fn unified_items_pagination_is_clamped() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            first: Some(99_999),
            skip: Some(10),
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, binds) = build_unified_items_sql(&filters, 0.5);
    assert!(sql.contains("LIMIT $"), "{sql}");
    assert!(sql.contains("OFFSET $"), "{sql}");
    let ints = bind_ints(&binds);
    assert!(ints.contains(&super::types::SHOP_MAX_PAGE_SIZE));
    assert!(ints.contains(&10));
}

#[test]
fn parse_unified_group_by_falls_back_to_listing() {
    let item = vec![("groupBy".to_string(), "item".to_string())];
    assert_eq!(parse_unified_group_by(&item), UnifiedGroupBy::Item);

    let listing = vec![("groupBy".to_string(), "listing".to_string())];
    assert_eq!(parse_unified_group_by(&listing), UnifiedGroupBy::Listing);

    let bogus = vec![("groupBy".to_string(), "bogus".to_string())];
    assert_eq!(parse_unified_group_by(&bogus), UnifiedGroupBy::Listing);
    assert_eq!(parse_unified_group_by(&[]), UnifiedGroupBy::Listing);
}

#[test]
fn parse_unified_filters_validates_source() {
    let pairs = vec![
        ("source".to_string(), "legacy".to_string()),
        ("sortBy".to_string(), "cheapest".to_string()),
    ];
    let f = parse_unified_filters(&pairs);
    assert_eq!(f.source, Some(UnifiedSource::Legacy));
    assert_eq!(f.base.sort_by, Some(ShopSortBy::Cheapest));

    let bad = vec![("source".to_string(), "bogus".to_string())];
    assert_eq!(parse_unified_filters(&bad).source, None);
    assert_eq!(parse_unified_filters(&[]).source, None);
}

#[test]
fn parse_unified_filters_validates_listing_type() {
    let primary = vec![("listingType".to_string(), "primary".to_string())];
    assert_eq!(
        parse_unified_filters(&primary).listing_type,
        Some(ShopListingType::Primary)
    );

    let secondary = vec![("listingType".to_string(), "secondary".to_string())];
    assert_eq!(
        parse_unified_filters(&secondary).listing_type,
        Some(ShopListingType::Secondary)
    );

    let bad = vec![("listingType".to_string(), "bogus".to_string())];
    assert_eq!(parse_unified_filters(&bad).listing_type, None);
    assert_eq!(parse_unified_filters(&[]).listing_type, None);
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.split(needle).count() - 1
}

/// Both encodings reach this feed (#391): its own comma-separated lists
/// (`rarity`, `wearableCategory`) and the repeated form /v1/items takes, which
/// is where `wearableGender` and its values come from. A caller reaching for
/// the wrong one used to get an unfiltered page that still looked filtered.
#[test]
fn parse_unified_filters_reads_both_wearable_gender_encodings() {
    let single = vec![("wearableGender".to_string(), "male".to_string())];
    assert_eq!(
        parse_unified_filters(&single).base.wearable_genders,
        ["male"]
    );

    let comma = vec![("wearableGender".to_string(), "male,female".to_string())];
    assert_eq!(
        parse_unified_filters(&comma).base.wearable_genders,
        ["male", "female"]
    );

    // The repeated form must not duplicate the first value, which the
    // comma-separated read also sees.
    let repeated = vec![
        ("wearableGender".to_string(), "male".to_string()),
        ("wearableGender".to_string(), "female".to_string()),
    ];
    assert_eq!(
        parse_unified_filters(&repeated).base.wearable_genders,
        ["male", "female"]
    );

    // A typo leaves the feed unfiltered rather than asking for a body shape no
    // item declares.
    let bogus = vec![("wearableGender".to_string(), "bogus".to_string())];
    assert!(parse_unified_filters(&bogus)
        .base
        .wearable_genders
        .is_empty());
    assert!(parse_unified_filters(&[]).base.wearable_genders.is_empty());
}

/// The param belongs to the unified feed alone, as upstream has it: the
/// per-listing shop feed and the trending rail never read it, so neither
/// changes shape here.
#[test]
fn wearable_gender_stays_off_the_shop_and_trending_feeds() {
    let pairs = vec![("wearableGender".to_string(), "male".to_string())];
    assert!(parse_shop_filters(&pairs).wearable_genders.is_empty());
    assert!(parse_trending_filters(&pairs)
        .filters
        .base
        .wearable_genders
        .is_empty());
}

/// Asserted by COUNTING the BOUND form, not by matching the column:
/// `search_wearable_body_shapes` already appears in every branch's gender
/// expression, so a plain `contains` would pass with no filter applied at all.
/// Only the filter binds its shapes; the gender expression uses literals.
#[test]
fn unified_wearable_gender_filter_lands_in_every_union_branch() {
    const BOUND_BODY_SHAPES: &str = "::text[] @> $";

    let (baseline, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert_eq!(occurrences(&baseline, BOUND_BODY_SHAPES), 0, "{baseline}");

    let male = unified_filters_for_genders(&["male"]);
    let (sql, binds) = build_unified_listings_sql(&male, 0.5);
    assert_eq!(occurrences(&sql, BOUND_BODY_SHAPES), 3, "{sql}");
    // `@>` is "declares all of", so ONE shape means "wearable BY a male avatar"
    // -- male-exclusive items plus unisex ones -- rather than male-exclusive
    // only. The one-element array is what makes that true.
    let arrays = bind_arrays(&binds);
    assert!(arrays.contains(&vec!["BaseMale".to_string()]), "{arrays:?}");
    assert!(
        !arrays.contains(&vec!["BaseMale".to_string(), "BaseFemale".to_string()]),
        "{arrays:?}"
    );

    // Both shapes required when both are asked for, and `unisex` IS that same
    // request -- an item declaring both is exactly what the outgoing `gender`
    // reports as unisex.
    for genders in [vec!["male", "female"], vec!["unisex"]] {
        let filters = unified_filters_for_genders(&genders);
        let (_, binds) = build_unified_listings_sql(&filters, 0.5);
        assert!(
            bind_arrays(&binds).contains(&vec!["BaseMale".to_string(), "BaseFemale".to_string()]),
            "{genders:?}"
        );
    }
}

/// The browse grid reads the grouped item feed, so the filter has to reach it
/// too: the feed is paginated and reports its own total, and dropping rows
/// client-side would return short pages under an overstated count.
#[test]
fn unified_wearable_gender_filter_applies_to_the_grouped_item_feed() {
    const BOUND_BODY_SHAPES: &str = "::text[] @> $";

    let filters = unified_filters_for_genders(&["female"]);
    let (sql, binds) = build_unified_items_sql(&filters, 0.5);
    assert_eq!(occurrences(&sql, BOUND_BODY_SHAPES), 3, "{sql}");
    assert!(
        bind_arrays(&binds).contains(&vec!["BaseFemale".to_string()]),
        "{binds:?}"
    );
}

fn unified_filters_for_genders(genders: &[&str]) -> UnifiedCatalogFilters {
    UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            wearable_genders: genders.iter().map(|g| g.to_string()).collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn unified_listing_type_filter_lands_in_every_union_branch() {
    const PRIMARY: &str = "AND mv.type = 'public_item_order'";
    const SECONDARY: &str = "AND mv.type <> 'public_item_order'";

    let (baseline_sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    let baseline = occurrences(&baseline_sql, PRIMARY);
    assert!(!baseline_sql.contains(SECONDARY), "{baseline_sql}");

    let primary = UnifiedCatalogFilters {
        listing_type: Some(ShopListingType::Primary),
        ..Default::default()
    };
    let (sql, _) = build_unified_listings_sql(&primary, 0.5);
    assert_eq!(occurrences(&sql, PRIMARY), baseline + 3, "{sql}");

    // The store branch's constant `type` is 'public_item_order', so a
    // resale-only request contradicts it and returns no store rows -- a mint
    // has no resale form.
    let secondary = UnifiedCatalogFilters {
        listing_type: Some(ShopListingType::Secondary),
        ..Default::default()
    };
    let (sql, _) = build_unified_listings_sql(&secondary, 0.5);
    assert_eq!(occurrences(&sql, SECONDARY), 3, "{sql}");
}

#[test]
fn unified_listing_type_filter_applies_to_the_grouped_item_feed() {
    const PRIMARY: &str = "AND mv.type = 'public_item_order'";

    let (baseline_sql, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    let baseline = occurrences(&baseline_sql, PRIMARY);

    let primary = UnifiedCatalogFilters {
        listing_type: Some(ShopListingType::Primary),
        ..Default::default()
    };
    let (sql, _) = build_unified_items_sql(&primary, 0.5);
    assert_eq!(occurrences(&sql, PRIMARY), baseline + 3, "{sql}");
}

// uint256 max: the squid's "no price set" sentinel on item.price.
const STORE_NO_PRICE_SENTINEL: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";
// 1e30 USD wei: the bound protecting CEIL(usd_wei/credit)::bigint from overflow.
const MAX_USD_WEI_BOUND: &str = "1000000000000000000000000000000";

/// The third UNION part of the default (both-sources) listings feed, i.e. the
/// CollectionStore branch plus the outer tail.
fn store_branch_part(sql: &str) -> &str {
    sql.split("UNION ALL")
        .nth(2)
        .expect("expected three unified branches")
}

#[test]
fn store_branch_reads_the_item_table_with_the_minting_predicates() {
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    let store = store_branch_part(&sql);
    assert!(store.contains(".item i"), "{store}");
    assert!(store.contains("i.search_is_store_minter = true"), "{store}");
    assert!(
        store.contains("i.search_is_collection_approved = true"),
        "{store}"
    );
    assert!(store.contains("i.available > 0"), "{store}");
    assert!(store.contains("i.price > 0"), "{store}");
    assert!(
        store.contains(&format!(
            "i.price IS DISTINCT FROM '{STORE_NO_PRICE_SENTINEL}'::numeric"
        )),
        "{store}"
    );
    assert!(
        store.contains("i.search_emote_outcome_type IS NULL"),
        "social emotes are hidden by the marketplace and must stay hidden here: {store}"
    );
    assert!(
        store.contains("i.network <> 'ETHEREUM'"),
        "CollectionStore.buy exists only on Polygon: {store}"
    );
    // Shaped like mv_trades so the shared joins/filters apply unchanged.
    assert!(
        store.contains("'public_item_order'::text AS type"),
        "{store}"
    );
    assert!(store.contains("NULL::jsonb AS assets"), "{store}");
}

#[test]
fn store_branch_skips_trade_only_predicates_but_takes_browse_filters() {
    let filters = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            rarities: vec!["rare".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, _) = build_unified_listings_sql(&filters, 0.5);
    let store = store_branch_part(&sql);
    assert!(store.contains("WHERE TRUE"), "{store}");
    assert!(
        !store.contains("mv.status = 'open'"),
        "the store relation has no status column: {store}"
    );
    assert!(
        !store.contains("trade_assets"),
        "a mint has no per-trade asset rows: {store}"
    );
    assert!(
        store.contains(
            "lower(COALESCE(item_p.rarity, item_s.rarity, nft.search_wearable_rarity)) = ANY($"
        ),
        "shared browse filters must reach the store branch: {store}"
    );
}

#[test]
fn store_branch_is_legacy_priced_with_the_shared_rate_bind() {
    let (sql, binds) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    let store = store_branch_part(&sql);
    assert!(store.contains("'legacy' AS source"), "{store}");
    assert!(store.contains("'store' AS acquisition"), "{store}");
    assert!(
        store.contains("(mv.amount_received::numeric * $1::numeric) AS usd_wei"),
        "same rate bind as the legacy trade branch: {store}"
    );
    assert!(
        store.contains("mv.amount_received::text AS mana_wei"),
        "{store}"
    );
    // The rate is bound once and shared: still exactly one text bind.
    assert_eq!(bind_texts(&binds).len(), 1);
}

#[test]
fn unified_id_and_enum_columns_are_text_cast_for_the_union() {
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    for cast in [
        "mv.id::text AS trade_id",
        "mv.type::text AS trade_type",
        "mv.sent_contract_address::text AS contract_address",
        "mv.sent_item_id::text AS item_id",
        "mv.sent_token_id::text AS token_id",
        "mv.network::text AS network",
    ] {
        assert_eq!(
            occurrences(&sql, cast),
            3,
            "uuid/varchar columns must merge across the UNION: {cast}: {sql}"
        );
    }
}

#[test]
fn unified_price_credits_cast_is_bounded_against_bigint_overflow() {
    let (sql, _) = build_unified_listings_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(
        sql.contains(&format!(
            "sub.usd_wei > 0 AND sub.usd_wei <= {MAX_USD_WEI_BOUND}::numeric"
        )),
        "one absurdly priced row must be dropped, not 500 the whole feed: {sql}"
    );

    let (sql, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(
        sql.contains(&format!(
            "u.usd_wei > 0 AND u.usd_wei <= {MAX_USD_WEI_BOUND}::numeric"
        )),
        "{sql}"
    );
}

#[test]
fn unified_items_break_price_ties_towards_the_signed_trade() {
    let (sql, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    const TIEBREAK: &str = "(CASE WHEN f.acquisition = 'trade' THEN 0 ELSE 1 END)";
    let price = sql.find("f.usd_wei ASC").expect("price sort");
    let tie = sql.find(TIEBREAK).expect("acquisition tiebreak");
    let stable = sql.find("f.trade_id").expect("stable tiebreak");
    assert!(
        price < tie && tie < stable,
        "trade-over-store must sit between price and the stable id: {sql}"
    );
}

mod rails;
