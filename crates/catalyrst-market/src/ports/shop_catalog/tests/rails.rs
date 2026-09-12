use super::*;

fn reference(
    category: &'static str,
    wearable_category: Option<&str>,
    rarity: Option<&str>,
) -> ReferenceItem {
    ReferenceItem {
        category,
        wearable_category: wearable_category.map(String::from),
        rarity: rarity.map(String::from),
    }
}

#[test]
fn reference_item_sql_looks_up_by_collection_and_blockchain_id() {
    let (sql, binds) = build_reference_item_sql("0xCollECTION", "3");
    assert!(sql.contains("squid_marketplace.item item"), "{sql}");
    assert!(sql.contains("item.collection_id = $"), "{sql}");
    assert!(sql.contains("item.blockchain_id = $"), "{sql}");
    assert!(sql.contains("::numeric"), "{sql}");
    assert!(sql.contains("LIMIT 1"), "{sql}");
    let texts = bind_texts(&binds);
    assert_eq!(texts, vec!["0xcollection".to_string(), "3".to_string()]);
}

#[test]
fn related_reuses_the_item_unified_core_for_one_card_per_item() {
    let (sql, _) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("wearable", Some("hat"), Some("rare")),
        None,
        0.5,
    );
    assert!(
        sql.contains("SELECT DISTINCT ON (f.contract_address, f.item_id)"),
        "{sql}"
    );
    assert!(
        sql.contains("COUNT(*) OVER (PARTITION BY u.contract_address, u.item_id) AS listing_count"),
        "{sql}"
    );
    assert!(
        sql.contains("CEIL(f.usd_wei / 100000000000000000::numeric)::bigint AS price_credits"),
        "{sql}"
    );
    assert_eq!(occurrences(&sql, "UNION ALL"), 2, "{sql}");
    assert!(sql.contains("i.search_is_store_minter = true"), "{sql}");
}

#[test]
fn related_hard_filters_on_the_anchor_category_and_subcategory() {
    let (sql, binds) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("wearable", Some("hat"), Some("rare")),
        None,
        0.5,
    );
    assert_eq!(
        occurrences(
            &sql,
            "COALESCE(item_p.item_type, item_s.item_type, nft.item_type) NOT ILIKE 'emote%'"
        ),
        3,
        "{sql}"
    );
    assert_eq!(
        occurrences(
            &sql,
            "search_wearable_category, item_s.search_wearable_category"
        ),
        3,
        "{sql}"
    );
    assert!(bind_arrays(&binds).contains(&vec!["hat".to_string()]));
}

#[test]
fn related_filters_emotes_to_emotes_when_the_anchor_is_an_emote() {
    let (sql, binds) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("emote", Some("dance"), Some("rare")),
        None,
        0.5,
    );
    assert_eq!(
        occurrences(
            &sql,
            "COALESCE(item_p.item_type, item_s.item_type, nft.item_type) ILIKE 'emote%'"
        ),
        3,
        "{sql}"
    );
    assert!(!sql.contains("NOT ILIKE 'emote%'"), "{sql}");
    assert!(bind_arrays(&binds).contains(&vec!["dance".to_string()]));
}

#[test]
fn related_falls_back_to_top_level_category_when_the_anchor_has_no_subcategory() {
    let (sql, binds) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("wearable", None, Some("rare")),
        None,
        0.5,
    );
    assert!(sql.contains("NOT ILIKE 'emote%'"), "{sql}");
    assert!(!sql.contains("= ANY("), "{sql}");
    assert!(bind_arrays(&binds).is_empty(), "{binds:?}");
}

#[test]
fn related_excludes_the_anchor_with_a_null_safe_disjunction() {
    let (sql, binds) = build_related_items_sql(
        "0xCollECTION",
        "3",
        &reference("wearable", Some("hat"), Some("rare")),
        None,
        0.5,
    );
    assert!(sql.contains("d.contract_address <> $"), "{sql}");
    assert!(sql.contains("COALESCE(d.item_id, '') <> $"), "{sql}");
    assert!(!sql.contains("NOT (d.contract_address"), "{sql}");
    let texts = bind_texts(&binds);
    assert!(texts.contains(&"0xcollection".to_string()), "{texts:?}");
    assert!(texts.contains(&"3".to_string()), "{texts:?}");
}

#[test]
fn related_orders_by_rarity_distance_then_recency_then_trade_id() {
    let (sql, _) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("wearable", Some("hat"), Some("rare")),
        None,
        0.5,
    );
    assert!(sql.contains("ORDER BY CASE lower(d.rarity)"), "{sql}");
    assert!(sql.contains("d.created_at DESC, d.trade_id"), "{sql}");
    assert!(sql.contains("WHEN 'rare' THEN 0"), "{sql}");
    assert!(sql.contains("WHEN 'uncommon' THEN 1"), "{sql}");
    assert!(sql.contains("WHEN 'epic' THEN 1"), "{sql}");
    assert!(sql.contains("WHEN 'common' THEN 2"), "{sql}");
    assert!(sql.contains("WHEN 'unique' THEN 5"), "{sql}");
    assert!(sql.contains("ELSE 8 END"), "{sql}");
}

#[test]
fn related_applies_no_rarity_preference_when_the_anchor_rarity_is_missing() {
    for anchor_rarity in [None, Some("not-a-rarity")] {
        let (sql, _) = build_related_items_sql(
            "0xcollection",
            "3",
            &reference("wearable", Some("hat"), anchor_rarity),
            None,
            0.5,
        );
        assert!(
            sql.contains("ORDER BY d.created_at DESC, d.trade_id"),
            "{sql}"
        );
        assert!(!sql.contains("CASE lower(d.rarity)"), "{sql}");
    }
}

#[test]
fn related_clamps_the_limit_to_the_rail_cap_and_never_paginates() {
    let anchor = reference("wearable", Some("hat"), Some("rare"));

    let (sql, binds) = build_related_items_sql("0xcollection", "3", &anchor, Some(9999), 0.5);
    assert!(sql.contains("LIMIT $"), "{sql}");
    assert!(!sql.contains("OFFSET"), "{sql}");
    assert!(!sql.contains("COUNT(*) OVER() AS total"), "{sql}");
    assert!(bind_ints(&binds).contains(&RELATED_MAX_LIMIT), "{binds:?}");

    let (_, binds) = build_related_items_sql("0xcollection", "3", &anchor, None, 0.5);
    assert!(
        bind_ints(&binds).contains(&RELATED_DEFAULT_LIMIT),
        "{binds:?}"
    );
}

#[test]
fn related_inherits_the_overflow_bound_and_trade_over_store_tiebreak_from_the_core() {
    let (sql, _) = build_related_items_sql(
        "0xcollection",
        "3",
        &reference("wearable", Some("hat"), Some("rare")),
        None,
        0.5,
    );
    assert!(
        sql.contains("u.usd_wei > 0 AND u.usd_wei <= 1000000000000000000000000000000::numeric"),
        "one absurd row must be dropped, not 500 the rail: {sql}"
    );
    let price = sql.find("f.usd_wei ASC").expect("price sort");
    let tie = sql
        .find("(CASE WHEN f.acquisition = 'trade' THEN 0 ELSE 1 END)")
        .expect("acquisition tiebreak");
    assert!(
        price < tie,
        "trade-over-store tiebreak must follow price: {sql}"
    );
}

/// The seller-attribution ranking it replaces never counts a primary mint
/// (marketplace-server #389).
#[test]
fn top_creators_attributes_to_the_item_creator() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(sql.contains("item.creator AS creator"), "{sql}");
    assert!(sql.contains("GROUP BY item.creator"), "{sql}");
    assert!(sql.contains("item.id = sale.item_id"), "{sql}");
    assert!(sql.contains("sale.item_id IS NOT NULL"), "{sql}");
    assert!(
        sql.contains("item.search_is_collection_approved = true"),
        "{sql}"
    );
}

/// In the scan's WHERE the window would bound BOTH counts and the all-time total would
/// silently become a second copy of the 30-day one -- a creator with 3,514 lifetime sales
/// introduced as having 62 (marketplace-server #390).
#[test]
fn top_creators_windows_the_ranking_count_without_windowing_the_lifetime_one() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(
        sql.contains("COUNT(*) FILTER (WHERE sale.timestamp >"),
        "{sql}"
    );
    assert!(sql.contains("COUNT(*)::int8 AS total_sales"), "{sql}");
    let join = sql.find("item.id = sale.item_id").expect("scan join");
    let group = sql.find("GROUP BY item.creator").expect("scan grouping");
    assert!(
        !sql[join..group].contains("sale.timestamp"),
        "window must not bound the scan: {sql}"
    );
}

/// Joined to `sale`, one item is one row PER SALE, so a creator's item count would come back
/// multiplied by how well it sold. LEFT + COALESCE, so a creator whose sales are visible but
/// whose catalogue is not still ranks.
#[test]
fn top_creators_counts_the_catalogue_separately_from_the_sales() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(
        sql.contains("COUNT(DISTINCT collection_id)::int8 AS collections"),
        "{sql}"
    );
    assert!(sql.contains("LEFT JOIN catalogue"), "{sql}");
    assert!(sql.contains("COALESCE(c.collections, 0)"), "{sql}");
}

/// The LEFT JOIN would otherwise let a zero-sale creator through. Both floors are enforced
/// in the query, next to each other -- a caller that has to re-filter can forget to.
#[test]
fn top_creators_floors_out_dormant_and_barely_published_creators() {
    let (sql, binds) = build_top_creators_sql(None, None);
    assert!(sql.contains("WHERE r.sales >="), "{sql}");
    assert!(sql.contains("COALESCE(c.items, 0) >="), "{sql}");
    let ints = bind_ints(&binds);
    assert!(ints.contains(&TOP_CREATORS_MIN_ITEMS), "{binds:?}");
    assert!(
        ints.contains(&TOP_CREATORS_MIN_SALES_PER_WINDOW),
        "default window asks the full rate: {binds:?}"
    );
}

/// Upstream measured the month's second-highest EARNING creator placing twelfth on unit
/// count, their items selling at roughly four times the field (#394).
#[test]
fn top_creators_rank_on_the_money_taken_not_the_unit_count() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(
        sql.contains(
            "COALESCE(SUM(sale.price::numeric) FILTER (WHERE sale.timestamp > $1), 0) AS volume"
        ),
        "{sql}"
    );
    assert!(
        sql.contains("ORDER BY r.volume DESC, r.creator ASC"),
        "{sql}"
    );
    assert!(!sql.contains("ORDER BY r.sales"), "{sql}");
}

/// Sorting the wei as TEXT is the trap: '900...' sorts above '1000...', ranking a creator by
/// the first digit of their revenue.
#[test]
fn top_creators_sort_the_revenue_numerically_and_expose_it_as_text() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(sql.contains("r.volume::text AS volume"), "{sql}");
    assert!(!sql.contains("ORDER BY r.volume::text"), "{sql}");
}

/// The case that forced a RATE: five sales is an ordinary month but an exceptional week, so
/// a FLAT five emptied a 7-day ranking outright on upstream's production data. A narrower
/// window has to yield a shorter row, not no row -- and the scaling has to bottom out above
/// one sale, or the floor stops preventing what it exists for.
#[test]
fn top_creators_scale_the_sales_floor_to_the_requested_window() {
    let (_, binds) = build_top_creators_sql(None, Some(18));
    let ints = bind_ints(&binds);
    assert!(ints.contains(&3), "{ints:?}");
    assert!(
        !ints.contains(&TOP_CREATORS_MIN_SALES_PER_WINDOW),
        "{ints:?}"
    );

    let (_, binds) = build_top_creators_sql(None, Some(90));
    assert!(bind_ints(&binds).contains(&15), "{binds:?}");

    let (_, binds) = build_top_creators_sql(None, Some(1));
    let ints = bind_ints(&binds);
    assert!(
        ints.contains(&TOP_CREATORS_MIN_WINDOW_SALES_FLOOR),
        "{ints:?}"
    );
    assert!(!ints.contains(&0), "{ints:?}");
}

#[test]
fn top_creators_binds_a_seconds_window_and_clamps() {
    use crate::ports::trendings::midnight_days_ago;

    let (_, binds) = build_top_creators_sql(None, None);
    let ints = bind_ints(&binds);
    let expected_default = midnight_days_ago(30);
    assert!(ints.contains(&expected_default), "seconds window: {ints:?}");
    assert!(!ints.contains(&(expected_default * 1000)), "{ints:?}");
    assert!(ints.contains(&30), "default row limit: {ints:?}");

    let (_, binds) = build_top_creators_sql(Some(9999), Some(9999));
    let ints = bind_ints(&binds);
    assert!(
        ints.contains(&midnight_days_ago(TOP_CREATORS_MAX_DAYS)),
        "{ints:?}"
    );
    assert!(ints.contains(&TOP_CREATORS_MAX_LIMIT), "{ints:?}");

    let (_, binds) = build_top_creators_sql(Some(0), Some(0));
    let ints = bind_ints(&binds);
    assert!(
        ints.contains(&midnight_days_ago(TOP_CREATORS_MIN_DAYS)),
        "{ints:?}"
    );
    assert!(ints.contains(&TOP_CREATORS_MIN_LIMIT), "{ints:?}");
}

#[test]
fn trending_windows_sales_over_the_shared_midnight_helper() {
    use crate::ports::shop_catalog::types::TRENDING_DEFAULT_DAYS;
    use crate::ports::trendings::midnight_days_ago;

    let (sql, binds) = build_trending_items_sql(None, None, &UnifiedCatalogFilters::default(), 0.5);
    assert!(
        bind_ints(&binds).contains(&midnight_days_ago(TRENDING_DEFAULT_DAYS)),
        "{binds:?}"
    );
    assert!(sql.contains("sale.timestamp > $1"), "{sql}");
    assert!(sql.contains("sale.search_item_id IS NOT NULL"), "{sql}");
    assert!(sql.contains("COUNT(*)::int8 AS sales"), "{sql}");
    assert!(sql.contains("SUM(sale.price::numeric) AS volume"), "{sql}");
    assert!(sql.contains("GROUP BY 1, 2"), "{sql}");
}

#[test]
fn trending_splits_slots_ceil_then_remainder() {
    let (_, binds) =
        build_trending_items_sql(Some(12), Some(1), &UnifiedCatalogFilters::default(), 0.5);
    let ints = bind_ints(&binds);
    assert!(ints.contains(&8), "sales slots: {ints:?}");
    assert!(ints.contains(&4), "volume slots: {ints:?}");
    assert!(ints.contains(&12), "limit: {ints:?}");
}

#[test]
fn trending_ranks_by_sales_then_fills_by_volume_in_a_total_order() {
    let (sql, _) =
        build_trending_items_sql(Some(10), Some(1), &UnifiedCatalogFilters::default(), 0.5);
    assert!(
        sql.contains(
            "ORDER BY listed.sales DESC, listed.volume DESC, listed.contract_address, listed.item_id"
        ),
        "{sql}"
    );
    assert!(
        sql.contains("PARTITION BY (ranked.sales_rank <= $"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "ORDER BY ranked.volume DESC, ranked.sales DESC, ranked.contract_address, ranked.item_id"
        ),
        "{sql}"
    );
    assert!(sql.contains("WHERE by_sales OR volume_rank <= $"), "{sql}");
    assert!(sql.contains("ORDER BY by_sales DESC,"), "{sql}");
    assert!(
        sql.contains("(CASE WHEN by_sales THEN sales_rank ELSE volume_rank END),"),
        "{sql}"
    );
    assert!(sql.contains("contract_address, item_id\nLIMIT $"), "{sql}");
}

#[test]
fn trending_draws_from_the_shared_item_unified_core() {
    let (sql, _) = build_trending_items_sql(None, None, &UnifiedCatalogFilters::default(), 0.5);
    assert!(
        sql.contains("DISTINCT ON (f.contract_address, f.item_id)"),
        "{sql}"
    );
    assert!(sql.contains("JOIN sales_window w"), "{sql}");
    assert!(
        sql.contains("ON w.contract_address = d.contract_address AND w.item_id = d.item_id"),
        "{sql}"
    );
    assert!(sql.contains("WHERE d.usd_wei > 0"), "{sql}");
}

#[test]
fn trending_clamps_first_and_days() {
    use crate::ports::trendings::midnight_days_ago;
    let (_, binds) =
        build_trending_items_sql(Some(999), Some(999), &UnifiedCatalogFilters::default(), 0.5);
    let ints = bind_ints(&binds);
    assert!(ints.contains(&50), "clamped limit: {ints:?}");
    assert!(ints.contains(&30), "{ints:?}");
    assert!(ints.contains(&20), "{ints:?}");
    assert!(ints.contains(&midnight_days_ago(7)), "{ints:?}");
}

#[test]
fn trending_defaults_to_a_week_at_the_cap() {
    use crate::ports::shop_catalog::types::{
        trending_clamp_days, TRENDING_DEFAULT_DAYS, TRENDING_MAX_DAYS,
    };
    use crate::ports::trendings::midnight_days_ago;
    assert_eq!(TRENDING_DEFAULT_DAYS, 7);
    assert_eq!(trending_clamp_days(None), TRENDING_MAX_DAYS);
    assert_eq!(trending_clamp_days(Some(1)), 1);
    let (_, binds) = build_trending_items_sql(None, None, &UnifiedCatalogFilters::default(), 0.5);
    let ints = bind_ints(&binds);
    assert!(ints.contains(&midnight_days_ago(7)), "{ints:?}");
    assert!(!ints.contains(&midnight_days_ago(1)), "{ints:?}");
}

#[test]
fn trending_carries_the_browse_filters_and_the_social_emote_flag() {
    let req = parse_trending_filters(&[
        ("category".into(), "emote".into()),
        ("rarity".into(), "legendary,mythic".into()),
        ("listingType".into(), "primary".into()),
        ("includeSocialEmotes".into(), "false".into()),
        ("first".into(), "5".into()),
        ("days".into(), "3".into()),
    ]);
    assert_eq!(req.first, Some(5));
    assert_eq!(req.days, Some(3));
    assert_eq!(req.filters.base.category.as_deref(), Some("emote"));
    assert_eq!(req.filters.base.rarities, vec!["legendary", "mythic"]);
    assert!(!req.filters.base.include_social_emotes);
    let (sql, _) = build_trending_items_sql(req.first, req.days, &req.filters, 0.5);
    assert!(
        sql.contains(
            "COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL"
        ),
        "social emotes excluded when the flag is false: {sql}"
    );
    assert!(sql.contains("mv.type = 'public_item_order'"), "{sql}");
}

#[test]
fn trending_defaults_to_including_social_emotes() {
    let req = parse_trending_filters(&[]);
    assert!(req.filters.base.include_social_emotes);
    let (sql, _) = build_trending_items_sql(req.first, req.days, &req.filters, 0.5);
    assert!(
        !sql.contains(
            "COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL"
        ),
        "no appended social-emote clause when the flag defaults true: {sql}"
    );
}

#[test]
fn unified_items_exclude_social_emotes_only_when_the_flag_is_false() {
    let (baseline, _) = build_unified_items_sql(&UnifiedCatalogFilters::default(), 0.5);
    assert!(
        !baseline.contains(
            "COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL"
        ),
        "default includes social emotes: {baseline}"
    );

    let excluded = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            include_social_emotes: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let (sql, _) = build_unified_items_sql(&excluded, 0.5);
    assert_eq!(
        sql.matches(
            "COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL"
        )
        .count(),
        3,
        "{sql}"
    );
}

#[test]
fn related_items_include_social_emotes_by_default() {
    let (sql, _) = build_related_items_sql(
        "0xCollection",
        "7",
        &reference("emote", None, Some("rare")),
        None,
        0.5,
    );
    assert!(
        !sql.contains(
            "COALESCE(item_p.search_emote_outcome_type, item_s.search_emote_outcome_type) IS NULL"
        ),
        "related rail must not drop social emotes by default: {sql}"
    );
}
