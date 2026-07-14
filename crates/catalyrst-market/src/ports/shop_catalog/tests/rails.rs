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
    // itemId is cast to numeric in SQL, not parsed in Rust, so an unbounded id survives.
    assert!(sql.contains("item.blockchain_id = $"), "{sql}");
    assert!(sql.contains("::numeric"), "{sql}");
    assert!(sql.contains("LIMIT 1"), "{sql}");
    let texts = bind_texts(&binds);
    // The contract is lowercased so a checksummed address from the URL matches the squid row.
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
    // native trade + legacy trade + CollectionStore mint reach the rail, so store
    // mints can surface here exactly as they do in the browse grid.
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
    // No sub-category means no wearable-category ANY() filter (related has no other array filters).
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
    // `NOT (contract = x AND item_id = y)` goes NULL when item_id is NULL and drops
    // the row, so the exclusion must be written as a disjunction.
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
    // Anchor is `rare` (rank 5): exact match distance 0, neighbours 1, then outwards.
    assert!(sql.contains("WHEN 'rare' THEN 0"), "{sql}");
    assert!(sql.contains("WHEN 'uncommon' THEN 1"), "{sql}");
    assert!(sql.contains("WHEN 'epic' THEN 1"), "{sql}");
    assert!(sql.contains("WHEN 'common' THEN 2"), "{sql}");
    assert!(sql.contains("WHEN 'unique' THEN 5"), "{sql}");
    // A row whose rarity is unknown sorts behind every real tier (past the widest gap of 7).
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

/// The creator rail attributes a sale to whoever CREATED the item, joining
/// `sale.item_id = item.id` -- the whole point, since the seller-attribution
/// ranking never counts a primary mint (marketplace-server #389).
#[test]
fn top_creators_attributes_to_the_item_creator() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(sql.contains("item.creator AS creator"), "{sql}");
    assert!(sql.contains("GROUP BY item.creator"), "{sql}");
    assert!(sql.contains("item.id = sale.item_id"), "{sql}");
    // Sales with no item cannot be attributed; unapproved collections are not
    // browsable so their creators are not introducible.
    assert!(sql.contains("sale.item_id IS NOT NULL"), "{sql}");
    assert!(
        sql.contains("item.search_is_collection_approved = true"),
        "{sql}"
    );
}

/// The window must be a FILTER on the ranking count, not part of the scan's
/// WHERE: there it would bound BOTH counts, and the all-time total would
/// silently become a second copy of the 30-day one -- a creator with 3,514
/// lifetime sales introduced as having 62 (marketplace-server #390).
#[test]
fn top_creators_windows_the_ranking_count_without_windowing_the_lifetime_one() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(
        sql.contains("COUNT(*) FILTER (WHERE sale.timestamp >"),
        "{sql}"
    );
    assert!(sql.contains("COUNT(*)::int8 AS total_sales"), "{sql}");
    // The scan's own WHERE -- everything between the join and the grouping. The
    // window must not be in it; matching on the whole query cannot tell that
    // clause apart from the FILTER's own `WHERE`.
    let join = sql.find("item.id = sale.item_id").expect("scan join");
    let group = sql.find("GROUP BY item.creator").expect("scan grouping");
    assert!(
        !sql[join..group].contains("sale.timestamp"),
        "window must not bound the scan: {sql}"
    );
}

/// Published counts come from `item` in their own CTE -- joined to `sale`, one
/// item is one row PER SALE, so a creator's item count would come back
/// multiplied by how well it sold. LEFT + COALESCE, so a creator whose sales
/// are visible but whose catalogue is not still ranks.
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

/// A window too thin to rank on is not "top" anything (the LEFT JOIN would let
/// a zero through), and a one-hit month over a near-empty catalogue is not
/// somewhere to send a shopper. Both floors are enforced in the query, next to
/// each other -- a caller that has to re-filter is a caller that can forget to.
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

/// The ranking is REVENUE, not units (#394): upstream measured the month's
/// second-highest EARNING creator placing twelfth on unit count, because their
/// items sell at roughly four times the field.
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

/// Sorting the wei as TEXT is the trap: '900...' sorts above '1000...', so a
/// creator would be ranked by the first digit of their revenue. The sum stays
/// numeric through the ORDER BY and is cast only on the way out.
#[test]
fn top_creators_sort_the_revenue_numerically_and_expose_it_as_text() {
    let (sql, _) = build_top_creators_sql(None, None);
    assert!(sql.contains("r.volume::text AS volume"), "{sql}");
    assert!(!sql.contains("ORDER BY r.volume::text"), "{sql}");
}

/// The sales floor is a RATE, and this is the case that forced it: five sales
/// is an ordinary month but an exceptional week, so a FLAT five emptied a
/// 7-day ranking outright on upstream's production data. A narrower window has
/// to yield a shorter row, not no row -- and the scaling has to bottom out
/// above one sale, or the floor stops preventing the thing it exists for.
#[test]
fn top_creators_scale_the_sales_floor_to_the_requested_window() {
    // 18 of the default 30 days -> three of the five sales: between the rate and
    // the absolute floor, where neither of them alone could produce the number.
    let (_, binds) = build_top_creators_sql(None, Some(18));
    let ints = bind_ints(&binds);
    assert!(ints.contains(&3), "{ints:?}");
    assert!(
        !ints.contains(&TOP_CREATORS_MIN_SALES_PER_WINDOW),
        "{ints:?}"
    );

    // A caller asking for a quarter is asking to be ranked on a quarter of
    // trading, not on a month's worth spread thin.
    let (_, binds) = build_top_creators_sql(None, Some(90));
    assert!(bind_ints(&binds).contains(&15), "{binds:?}");

    // At a one-day window the rate rounds to zero, and a floor of zero ranks a
    // creator on the price of their single sale.
    let (_, binds) = build_top_creators_sql(None, Some(1));
    let ints = bind_ints(&binds);
    assert!(
        ints.contains(&TOP_CREATORS_MIN_WINDOW_SALES_FLOOR),
        "{ints:?}"
    );
    assert!(!ints.contains(&0), "{ints:?}");
}

/// The window is bound as a unix SECONDS anchor (not milliseconds) and both the
/// row count and window are clamped to the supported range.
#[test]
fn top_creators_binds_a_seconds_window_and_clamps() {
    use crate::ports::trendings::midnight_days_ago;

    // Defaults: 30-day window (seconds), 30-row limit.
    let (_, binds) = build_top_creators_sql(None, None);
    let ints = bind_ints(&binds);
    let expected_default = midnight_days_ago(30);
    assert!(ints.contains(&expected_default), "seconds window: {ints:?}");
    // A milliseconds bound would be ~1000x larger and match every sale ever.
    assert!(!ints.contains(&(expected_default * 1000)), "{ints:?}");
    assert!(ints.contains(&30), "default row limit: {ints:?}");

    // Over the max clamps to the max window + max rows.
    let (_, binds) = build_top_creators_sql(Some(9999), Some(9999));
    let ints = bind_ints(&binds);
    assert!(
        ints.contains(&midnight_days_ago(TOP_CREATORS_MAX_DAYS)),
        "{ints:?}"
    );
    assert!(ints.contains(&TOP_CREATORS_MAX_LIMIT), "{ints:?}");

    // Under the min clamps to the min window + min rows.
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
    // The window anchor is bound as unix SECONDS via the SAME helper /v1/trendings and
    // top-creators use, so all three span the same slice of history.
    assert!(
        bind_ints(&binds).contains(&midnight_days_ago(TRENDING_DEFAULT_DAYS)),
        "{binds:?}"
    );
    assert!(sql.contains("sale.timestamp > $1"), "{sql}");
    // A sale with no item identity cannot be ranked, so it never reaches the join.
    assert!(sql.contains("sale.search_item_id IS NOT NULL"), "{sql}");
    assert!(sql.contains("COUNT(*)::int8 AS sales"), "{sql}");
    // Volume is what the sales SETTLED at, not the current price times the count.
    assert!(sql.contains("SUM(sale.price::numeric) AS volume"), "{sql}");
    assert!(sql.contains("GROUP BY 1, 2"), "{sql}");
}

#[test]
fn trending_splits_slots_ceil_then_remainder() {
    // 12 slots -> ceil(12 * 0.6) = 8 by sales, 12 - 8 = 4 by volume. Ceil + remainder,
    // not two independent fractional slices that would under-fill to 11.
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
    // Sales pass: rank by (sales, volume) then the unique item key.
    assert!(
        sql.contains(
            "ORDER BY listed.sales DESC, listed.volume DESC, listed.contract_address, listed.item_id"
        ),
        "{sql}"
    );
    // Fill pass: rank the leftovers by (volume, sales), partitioned on the same by_sales
    // predicate so it ranks exactly what the sales pass did not take.
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
    // Sales block first (false < true, so DESC leads), each block in its own rank order,
    // then the unique item key as the total-order tiebreak.
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
    // Same DISTINCT-ON core as the browse grid and the related rail, joined to the window.
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
    // first clamps to TRENDING_MAX_LIMIT (50), days to TRENDING_MAX_DAYS (7).
    use crate::ports::trendings::midnight_days_ago;
    let (_, binds) =
        build_trending_items_sql(Some(999), Some(999), &UnifiedCatalogFilters::default(), 0.5);
    let ints = bind_ints(&binds);
    assert!(ints.contains(&50), "clamped limit: {ints:?}");
    // ceil(50 * 0.6) = 30 by sales, 20 by volume.
    assert!(ints.contains(&30), "{ints:?}");
    assert!(ints.contains(&20), "{ints:?}");
    // Window anchored 7 days ago, not 999.
    assert!(ints.contains(&midnight_days_ago(7)), "{ints:?}");
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
    // listingType=primary lands in every trade branch of the core.
    assert!(sql.contains("mv.type = 'public_item_order'"), "{sql}");
}

#[test]
fn trending_defaults_to_including_social_emotes() {
    let req = parse_trending_filters(&[]);
    assert!(req.filters.base.include_social_emotes);
    let (sql, _) = build_trending_items_sql(req.first, req.days, &req.filters, 0.5);
    // The store branch base relation always names `search_emote_outcome_type`; the flag only
    // controls the appended COALESCE clause, so assert on that specific clause.
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
    // The store branch base relation names `search_emote_outcome_type` unconditionally; the
    // flag only governs the appended COALESCE clause.
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
    // COALESCE over both item joins, once per union branch (native trade + legacy trade +
    // store); the store branch already excludes social emotes at its base relation, so the
    // COALESCE clause is a no-op there rather than a second rule.
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
