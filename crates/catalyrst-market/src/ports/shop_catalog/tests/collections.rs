use super::*;

/// Upstream 66c9fd1: one param, two shapes. The comma form is what a seasonal event needs --
/// it tags whole collections and routinely names dozens.
#[test]
fn contract_address_set_reads_the_comma_repeated_and_bracket_forms() {
    let a = "0x1111111111111111111111111111111111111111";
    let b = "0x2222222222222222222222222222222222222222";

    let comma = vec![("contractAddress".to_string(), format!("{a},{b}"))];
    assert_eq!(
        parse_unified_filters(&comma).contract_addresses,
        Some(vec![a.to_string(), b.to_string()])
    );

    let repeated = vec![
        ("contractAddress".to_string(), a.to_string()),
        ("contractAddress".to_string(), b.to_string()),
    ];
    assert_eq!(
        parse_unified_filters(&repeated).contract_addresses,
        Some(vec![a.to_string(), b.to_string()])
    );

    let bracket = vec![
        ("contractAddress[]".to_string(), a.to_string()),
        ("contractAddress[]".to_string(), b.to_string()),
    ];
    assert_eq!(
        parse_unified_filters(&bracket).contract_addresses,
        Some(vec![a.to_string(), b.to_string()])
    );
}

#[test]
fn contract_address_set_lowercases_and_drops_non_addresses() {
    let a = "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let pairs = vec![(
        "contractAddress".to_string(),
        format!(" {a} , not-an-address ,, "),
    )];
    assert_eq!(
        parse_unified_filters(&pairs).contract_addresses,
        Some(vec![a.to_lowercase()])
    );
}

/// `None` and `Some(vec![])` are NOT the same: a blank value has always meant "absent" here,
/// while a set the caller named that resolved to nothing must yield an empty page.
#[test]
fn a_blank_contract_address_is_absent_and_an_all_invalid_set_is_empty() {
    let blank = vec![("contractAddress".to_string(), "  ".to_string())];
    assert_eq!(parse_unified_filters(&blank).contract_addresses, None);
    assert_eq!(parse_unified_filters(&[]).contract_addresses, None);

    let invalid = vec![("contractAddress".to_string(), "nope,also-nope".to_string())];
    assert_eq!(
        parse_unified_filters(&invalid).contract_addresses,
        Some(Vec::new())
    );
}

/// Upstream drops the singular filter whenever the set is present -- one address included, since
/// it parses as a one-element set. ANDing the two would collapse a multi-address request to
/// "the first address only".
#[test]
fn the_singular_contract_address_is_dropped_when_a_set_is_present() {
    let a = "0x1111111111111111111111111111111111111111";
    let b = "0x2222222222222222222222222222222222222222";
    let pairs = vec![
        ("contractAddress".to_string(), a.to_string()),
        ("contractAddress".to_string(), b.to_string()),
    ];
    let filters = parse_unified_filters(&pairs);
    assert_eq!(filters.base.contract_address, None);

    let single = vec![("contractAddress".to_string(), a.to_string())];
    let filters = parse_unified_filters(&single);
    assert_eq!(filters.base.contract_address, None);
    assert_eq!(filters.contract_addresses, Some(vec![a.to_string()]));
}

/// The trending rail never reads the set, as upstream's trending handler does not either.
#[test]
fn the_trending_rail_ignores_a_collection_set() {
    let pairs = vec![(
        "contractAddress".to_string(),
        "0x1111111111111111111111111111111111111111".to_string(),
    )];
    assert_eq!(
        parse_trending_filters(&pairs).filters.contract_addresses,
        None
    );
}

pub(super) const UNION_BRANCHES: usize = 3;

fn unified_with_set(set: Option<Vec<String>>) -> (String, Vec<Bind>) {
    build_unified_listings_sql(
        &UnifiedCatalogFilters {
            contract_addresses: set,
            ..Default::default()
        },
        0.5,
    )
}

#[test]
fn a_collection_set_reaches_every_union_branch() {
    let a = "0x1111111111111111111111111111111111111111";
    let b = "0x2222222222222222222222222222222222222222";
    let (sql, binds) = unified_with_set(Some(vec![a.to_string(), b.to_string()]));
    assert_eq!(
        occurrences(&sql, "mv.sent_contract_address = ANY("),
        UNION_BRANCHES,
        "{sql}"
    );
    assert!(
        bind_arrays(&binds).contains(&vec![a.to_string(), b.to_string()]),
        "{binds:?}"
    );
}

/// Fails CLOSED. Falling through to "no filter" would serve the entire catalogue to a caller
/// whose event filter merely failed to resolve, which looks like a working event.
#[test]
fn an_empty_collection_set_empties_every_union_branch() {
    let (sql, _) = unified_with_set(Some(Vec::new()));
    assert_eq!(occurrences(&sql, "AND FALSE"), UNION_BRANCHES, "{sql}");
    assert!(!sql.contains("mv.sent_contract_address = ANY("), "{sql}");
}

#[test]
fn no_collection_set_leaves_the_feed_unfiltered() {
    let (sql, _) = unified_with_set(None);
    assert!(!sql.contains("mv.sent_contract_address = ANY("), "{sql}");
    assert!(!sql.contains("AND FALSE"), "{sql}");
}

/// The set lives on the unified filters, not the shared base, so the plain shop feed can never
/// see it -- exactly upstream's reason for declaring it there.
#[test]
fn the_shop_feed_keeps_its_singular_collection_filter() {
    let (sql, binds) = build_shop_listings_sql(&ShopCatalogFilters {
        contract_address: Some("0xABCdef".to_string()),
        ..Default::default()
    });
    assert!(sql.contains("mv.sent_contract_address = $"), "{sql}");
    assert!(!sql.contains("mv.sent_contract_address = ANY("), "{sql}");
    assert!(bind_texts(&binds).contains(&"0xabcdef".to_string()));
}
