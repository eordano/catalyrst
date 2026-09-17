use catalyrst_market::logic::catalog::parse_catalog_filters;
use catalyrst_market::ports::catalog::{
    build_collections_items_catalog_query, build_collections_items_count_query,
};

#[test]
#[ignore]
fn print_browse_default_sql() {
    let pairs = vec![("first".to_string(), "24".to_string())];
    let filters = parse_catalog_filters(&pairs, false).expect("filters");
    let (items_sql, _) = build_collections_items_catalog_query(&filters);
    let (count_sql, _) = build_collections_items_count_query(&filters);
    println!("=== ITEMS SQL ===\n{items_sql}\n=== COUNT SQL ===\n{count_sql}");
}

#[test]
#[ignore]
fn print_nfts_wearable_sql() {
    use catalyrst_market::ports::nfts::{build_nfts_query, Bind, NftFilters};
    let filters = NftFilters {
        first: Some(24),
        category: Some(catalyrst_market::dcl_schemas::NftCategory::Wearable),
        ..Default::default()
    };
    for for_count in [false, true] {
        let (sql, binds) = build_nfts_query(&filters, for_count);
        println!("=== NFTS SQL (count={for_count}) ===\n{sql}\n--- binds:");
        for (i, b) in binds.iter().enumerate() {
            match b {
                Bind::Text(v) => println!("${} = '{}'", i + 1, v),
                Bind::TextArray(v) => println!("${} = {:?}", i + 1, v),
                Bind::Int(v) => println!("${} = {}", i + 1, v),
                Bind::Float(v) => println!("${} = {}", i + 1, v),
            }
        }
    }
}

/// The shop feeds carry the widest SQL in the crate (a coupon LATERAL, a three-way UNION and
/// two wrapper layers), so a printer here is what lets one PREPARE them against a scratch
/// database and catch a column reference the string assertions cannot.
#[test]
#[ignore]
fn print_shop_catalog_sql() {
    use catalyrst_market::ports::shop_catalog::{
        build_shop_listings_sql, build_unified_items_sql, build_unified_listings_sql,
        ShopCatalogFilters, UnifiedCatalogFilters,
    };

    let (shop_sql, _) = build_shop_listings_sql(&ShopCatalogFilters {
        discounted: Some(true),
        min_price_credits: Some(3.0),
        ..Default::default()
    });
    println!("=== SHOP SQL ===\n{shop_sql}");

    let unified = UnifiedCatalogFilters {
        base: ShopCatalogFilters {
            discounted: Some(true),
            ..Default::default()
        },
        contract_addresses: Some(vec![
            "0x1111111111111111111111111111111111111111".to_string()
        ]),
        ..Default::default()
    };
    let (listings_sql, _) = build_unified_listings_sql(&unified, 0.5);
    println!("=== UNIFIED LISTINGS SQL ===\n{listings_sql}");
    let (items_sql, _) = build_unified_items_sql(&unified, 0.5);
    println!("=== UNIFIED ITEMS SQL ===\n{items_sql}");
}

/// Upstream 83efd20 replaced the plain trades join with a LATERAL over a renamed CTE. Printing
/// both item feeds is what lets one PREPARE them and prove the rename reached every reference.
#[test]
#[ignore]
fn print_item_feeds_sql() {
    use catalyrst_market::dcl_schemas::NftCategory;
    use catalyrst_market::ports::items::{build_catalog_items_query, build_items_query};
    use catalyrst_market::ports::items::{CatalogItemsParams, ItemFilters};

    let filters = ItemFilters {
        first: Some(24),
        category: Some(NftCategory::Wearable),
        ..Default::default()
    };
    let (items_sql, _) = build_items_query(&filters);
    println!("=== ITEMS SQL ===\n{items_sql}");
    let (catalog_sql, _) =
        build_catalog_items_query(&filters, &CatalogItemsParams::default(), "0.5");
    println!("=== CATALOG ITEMS SQL ===\n{catalog_sql}");
}
