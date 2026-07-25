use super::component::{network_and_chain, top_level_category};
use super::sql::{
    build_importable_listings_sql, build_legacy_listings_sql, build_shop_listings_sql,
    credits_to_wei, escape_like, to_credits, Bind, ASSET_TYPE_ERC20, ASSET_TYPE_USD_PEGGED_MANA,
    USD_WEI_PER_CREDIT,
};
use super::types::{parse_shop_filters, LegacyCatalogFilters, ShopCatalogFilters, ShopSortBy};
use super::unified::{
    build_unified_listings_sql, parse_unified_filters, unified_min_price_bound_wei,
    UnifiedCatalogFilters, UnifiedSource,
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
    assert!(sql.contains("UNION ALL"), "{sql}");
    assert!(sql.contains("'native' AS source"), "{sql}");
    assert!(sql.contains("'legacy' AS source"), "{sql}");
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
fn unified_source_filter_builds_a_single_branch() {
    let native_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Native),
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&native_only, 0.5);
    assert!(!sql.contains("UNION ALL"), "{sql}");
    assert!(sql.contains("'native' AS source"), "{sql}");
    assert!(!sql.contains("'legacy' AS source"), "{sql}");
    assert!(
        bind_texts(&binds).is_empty(),
        "no rate bind for native-only"
    );

    let legacy_only = UnifiedCatalogFilters {
        source: Some(UnifiedSource::Legacy),
        ..Default::default()
    };
    let (sql, binds) = build_unified_listings_sql(&legacy_only, 0.5);
    assert!(!sql.contains("UNION ALL"), "{sql}");
    assert!(!sql.contains("'native' AS source"), "{sql}");
    assert!(sql.contains("'legacy' AS source"), "{sql}");
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
        2,
        "{sql}"
    );
    assert_eq!(
        sql.matches("COALESCE(item_p.item_type, item_s.item_type, nft.item_type) ILIKE 'emote%'")
            .count(),
        2,
        "{sql}"
    );
    let texts = bind_texts(&binds);
    assert_eq!(
        texts.iter().filter(|t| *t == "0xabc").count(),
        2,
        "{texts:?}"
    );
    assert_eq!(
        texts.iter().filter(|t| *t == "%dance%").count(),
        2,
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
