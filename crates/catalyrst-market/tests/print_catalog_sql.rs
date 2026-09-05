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
