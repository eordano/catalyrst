use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;

use crate::dcl_schemas::{ChainId, Network};
use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip};
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleSortBy {
    MostExpensive,
    RecentlySold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleType {
    Order,
    Bid,
    Mint,
}

impl SaleType {
    fn as_str(&self) -> &'static str {
        match self {
            SaleType::Order => "order",
            SaleType::Bid => "bid",
            SaleType::Mint => "mint",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SaleFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<SaleSortBy>,
    pub sale_type: Option<SaleType>,
    pub categories: Vec<String>,
    pub seller: Option<String>,
    pub buyer: Option<String>,
    pub contract_address: Option<String>,
    pub token_id: Option<String>,
    pub item_id: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
    pub network: Option<Network>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Sale {
    pub id: String,
    #[serde(rename = "itemId")]
    pub item_id: Option<String>,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub buyer: String,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    pub network: Network,
    pub price: String,
    pub seller: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub timestamp: i64,
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "type")]
    #[cfg_attr(feature = "ts", ts(rename = "type"))]
    pub sale_type: String,
}

pub struct SalesComponent {
    pool: PgPool,
}

impl SalesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_sales(&self, f: &SaleFilters) -> Result<(Vec<Sale>, i64), ApiError> {
        let limit = clamp_first(f.first, 100);
        let offset = clamp_skip(f.skip);
        let order_by = match f.sort_by {
            Some(SaleSortBy::MostExpensive) => "sort_price DESC",
            _ => "sort_timestamp DESC",
        };

        let mut where_parts: Vec<String> = Vec::new();
        let mut bind_str: Vec<String> = Vec::new();
        let mut bind_i64: Vec<i64> = Vec::new();

        let mut kinds: Vec<char> = Vec::new();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            format!("${}", idx)
        };

        if let Some(ref v) = f.sale_type {
            where_parts.push(format!("type = {}", next()));
            bind_str.push(v.as_str().to_string());
            kinds.push('s');
        }
        if let Some(ref v) = f.buyer {
            where_parts.push(format!("buyer = {}", next()));
            bind_str.push(v.clone());
            kinds.push('s');
        }
        if let Some(ref v) = f.seller {
            where_parts.push(format!("seller = {}", next()));
            bind_str.push(v.to_lowercase());
            kinds.push('s');
        }
        if let Some(ref v) = f.contract_address {
            where_parts.push(format!("search_contract_address = {}", next()));
            bind_str.push(v.to_lowercase());
            kinds.push('s');
        }
        if let Some(ref v) = f.item_id {
            where_parts.push(format!("search_item_id = {}", next()));
            bind_str.push(v.clone());
            kinds.push('s');
        }
        if let Some(ref v) = f.token_id {
            where_parts.push(format!("search_token_id = {}", next()));
            bind_str.push(v.clone());
            kinds.push('s');
        }
        if let Some(ref v) = f.min_price {
            where_parts.push(format!("price >= {}::numeric", next()));
            bind_str.push(v.clone());
            kinds.push('s');
        }
        if let Some(ref v) = f.max_price {
            where_parts.push(format!("price <= {}::numeric", next()));
            bind_str.push(v.clone());
            kinds.push('s');
        }
        if !f.categories.is_empty() {
            where_parts.push(format!("search_category = ANY({}::text[])", next()));
            bind_str.push(format!("{{{}}}", f.categories.join(",")));
            kinds.push('s');
        }

        if let Some(net) = f.network {
            let db_nets: &[&str] = match net {
                Network::Ethereum => &["ETHEREUM"],
                Network::Matic => &["MATIC", "POLYGON"],
            };
            where_parts.push(format!("network = ANY({}::text[])", next()));
            bind_str.push(format!("{{{}}}", db_nets.join(",")));
            kinds.push('s');
        }
        if let Some(v) = f.from {
            where_parts.push(format!("(timestamp * 1000) >= {}", next()));
            bind_i64.push(v);
            kinds.push('i');
        }
        if let Some(v) = f.to {
            where_parts.push(format!("(timestamp * 1000) <= {}", next()));
            bind_i64.push(v);
            kinds.push('i');
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let limit_p = next();
        let offset_p = next();

        let inner = format!(
            r#"
  SELECT
    id,
    type,
    buyer,
    seller,
    search_item_id::text       AS item_id,
    search_token_id::text      AS token_id,
    search_contract_address    AS contract_address,
    price::text                AS price,
    (timestamp * 1000)::bigint AS timestamp,
    tx_hash,
    network,
    search_category            AS category,
    timestamp                  AS sort_timestamp,
    price                      AS sort_price
  FROM {schema}.sale
  {where_clause}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_clause = where_clause,
        );

        let page_sql = format!(
            "SELECT legacy_sales.* FROM ({inner}) AS legacy_sales \
             ORDER BY {order_by} LIMIT {limit_p} OFFSET {offset_p}"
        );
        let count_sql =
            format!("SELECT COUNT(*)::int8 AS sales_count FROM ({inner}) AS legacy_sales");

        let index_sort = order_by.starts_with("sort_timestamp");

        let bind_str_p = bind_str.clone();
        let bind_i64_p = bind_i64.clone();
        let kinds_p = kinds.clone();
        let bind_str_c = bind_str.clone();
        let bind_i64_c = bind_i64.clone();
        let kinds_c = kinds.clone();
        let page_pool = self.pool.clone();
        let count_pool = self.pool.clone();

        let page_fut = async move {
            let mut tx = page_pool.begin().await?;
            sqlx::query("SET LOCAL random_page_cost = 1.1")
                .execute(&mut *tx)
                .await?;
            if index_sort {
                sqlx::query("SET LOCAL enable_sort = off")
                    .execute(&mut *tx)
                    .await?;
            }
            let mut q = sqlx::query(sqlx::AssertSqlSafe(page_sql));
            let mut si = bind_str_p.iter();
            let mut ii = bind_i64_p.iter();
            for k in &kinds_p {
                if *k == 's' {
                    q = q.bind(si.next().cloned().unwrap_or_default());
                } else {
                    q = q.bind(*ii.next().unwrap_or(&0));
                }
            }
            q = q.bind(limit).bind(offset);
            let rows = q.fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>(rows)
        };
        let count_fut = async move {
            let mut tx = count_pool.begin().await?;
            sqlx::query("SET LOCAL random_page_cost = 1.1")
                .execute(&mut *tx)
                .await?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(count_sql));
            let mut si = bind_str_c.iter();
            let mut ii = bind_i64_c.iter();
            for k in &kinds_c {
                if *k == 's' {
                    q = q.bind(si.next().cloned().unwrap_or_default());
                } else {
                    q = q.bind(*ii.next().unwrap_or(&0));
                }
            }
            let row = q.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>(row.try_get::<i64, _>("sales_count").unwrap_or(0))
        };

        let (rows, total) = tokio::try_join!(page_fut, count_fut)?;
        let sales: Vec<Sale> = rows
            .into_iter()
            .map(|r| {
                let network_str: String = r.try_get("network").unwrap_or_default();
                let (network, chain_id) = crate::ports::orders::network_and_chain(&network_str);
                Sale {
                    id: r.try_get("id").unwrap_or_default(),
                    item_id: r.try_get::<Option<String>, _>("item_id").unwrap_or(None),
                    contract_address: r.try_get("contract_address").unwrap_or_default(),
                    buyer: r.try_get("buyer").unwrap_or_default(),
                    chain_id,
                    network,
                    price: r.try_get("price").unwrap_or_default(),
                    seller: r.try_get("seller").unwrap_or_default(),
                    timestamp: r.try_get::<i64, _>("timestamp").unwrap_or(0),
                    token_id: r.try_get::<Option<String>, _>("token_id").unwrap_or(None),
                    tx_hash: r.try_get("tx_hash").unwrap_or_default(),
                    sale_type: r.try_get("type").unwrap_or_default(),
                }
            })
            .collect();
        Ok((sales, total))
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<SaleFilters, InvalidParameterError> {
    let p = Params::new(pairs);

    let sort_by = p
        .get_value("sortBy", &["most_expensive", "recently_sold"], None)
        .map(|s| match s.as_str() {
            "most_expensive" => SaleSortBy::MostExpensive,
            _ => SaleSortBy::RecentlySold,
        });

    let sale_type = p
        .get_value("type", &["order", "bid", "mint"], None)
        .map(|s| match s.as_str() {
            "bid" => SaleType::Bid,
            "mint" => SaleType::Mint,
            _ => SaleType::Order,
        });

    let categories = p.get_list(
        "category",
        &["parcel", "estate", "wearable", "ens", "emote"],
    );

    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            _ => Network::Matic,
        });

    Ok(SaleFilters {
        first: p.get_number("first", None).map(|v| v as i64),
        skip: p.get_number("skip", None).map(|v| v as i64),
        sort_by,
        sale_type,
        categories,
        seller: p.get_address("seller", true, None),
        buyer: p.get_address("buyer", true, None),
        contract_address: p.get_address("contractAddress", true, None),
        token_id: p.get_string("tokenId", None),
        item_id: p.get_string("itemId", None),
        from: p.get_number("from", None).map(|v| v as i64),
        to: p.get_number("to", None).map(|v| v as i64),
        min_price: p.get_string("minPrice", None),
        max_price: p.get_string("maxPrice", None),
        network,
    })
}

#[derive(Debug, Clone, Default)]
pub struct SalesSummaryFilters {
    pub seller: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SalesSummaryCollection {
    pub contract_address: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sold: i64,
    pub earned_wei: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SalesSummaryItem {
    pub contract_address: String,
    pub item_id: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sold_lifetime: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SalesSummaryRoyalties {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub resales: i64,
    pub volume_wei: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct SalesSummary {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub mints: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub resales: i64,
    pub earned_wei: String,
    pub by_collection: Vec<SalesSummaryCollection>,
    pub by_item: Vec<SalesSummaryItem>,
    pub royalties: SalesSummaryRoyalties,
}

/// The window is compared against the stored seconds rather than multiplying every row, so the
/// sale timestamp index still answers the scan.
fn summary_window(f: &SalesSummaryFilters) -> String {
    let mut window = String::new();
    if f.from.is_some() {
        window.push_str(" AND timestamp >= $2::numeric / 1000");
    }
    if f.to.is_some() {
        let param = if f.from.is_some() { "$3" } else { "$2" };
        window.push_str(&format!(" AND timestamp <= {param}::numeric / 1000"));
    }
    window
}

pub fn sales_summary_sql(f: &SalesSummaryFilters) -> String {
    let window = summary_window(f);
    format!(
        r#"
WITH seller_sales AS (
  SELECT type, price, timestamp, search_contract_address, search_item_id
  FROM {schema}.sale WHERE seller = $1
), window_sales AS (
  SELECT * FROM seller_sales WHERE TRUE{window}
), collections AS (
  SELECT search_contract_address, COUNT(*) AS sold, SUM(price)::text AS earned
  FROM window_sales GROUP BY search_contract_address
), items AS (
  SELECT search_contract_address, search_item_id, COUNT(*) AS sold
  FROM seller_sales
  WHERE type = 'mint' AND search_item_id IS NOT NULL
  GROUP BY search_contract_address, search_item_id
), royalties AS (
  SELECT COUNT(*) AS resales, COALESCE(SUM(price), 0)::text AS volume
  FROM {schema}.sale s
  WHERE s.type IN ('order', 'bid')
    AND EXISTS (
      SELECT 1 FROM {schema}.item i
      WHERE i.id = s.item_id AND LOWER(i.creator) = $1
    ){window}
)
SELECT json_build_object(
  'total', COUNT(*),
  'mints', COUNT(*) FILTER (WHERE type = 'mint'),
  'resales', COUNT(*) FILTER (WHERE type IN ('order', 'bid')),
  'earnedWei', COALESCE(SUM(price), 0)::text,
  'byCollection', (SELECT COALESCE(json_agg(json_build_object(
    'contractAddress', search_contract_address, 'sold', sold, 'earnedWei', earned
  ) ORDER BY search_contract_address), '[]'::json) FROM collections),
  'byItem', (SELECT COALESCE(json_agg(json_build_object(
    'contractAddress', search_contract_address, 'itemId', search_item_id::text, 'soldLifetime', sold
  ) ORDER BY search_contract_address, search_item_id), '[]'::json) FROM items),
  'royalties', (SELECT json_build_object('resales', resales, 'volumeWei', volume) FROM royalties)
) AS summary FROM window_sales
"#,
        schema = MARKETPLACE_SQUID_SCHEMA,
        window = window,
    )
}

impl SalesComponent {
    pub async fn get_summary(&self, f: &SalesSummaryFilters) -> Result<SalesSummary, ApiError> {
        let mut q =
            sqlx::query(sqlx::AssertSqlSafe(sales_summary_sql(f))).bind(f.seller.to_lowercase());
        for bound in [f.from, f.to].into_iter().flatten() {
            q = q.bind(bound);
        }
        let row = q.fetch_one(&self.pool).await?;
        let summary: serde_json::Value = row.try_get("summary")?;
        serde_json::from_value(summary)
            .map_err(|e| ApiError::internal(format!("sales summary shape: {e}")))
    }
}

pub fn parse_summary_filters(pairs: &[(String, String)]) -> Result<SalesSummaryFilters, ApiError> {
    let p = Params::new(pairs);
    let seller = p
        .get_address("seller", true, None)
        .ok_or_else(|| ApiError::bad_request("A valid seller address is required"))?;

    let mut bounds: [Option<i64>; 2] = [None, None];
    for (slot, key) in ["from", "to"].iter().enumerate() {
        if let Some(raw) = p.get_string(key, None) {
            let parsed = raw
                .parse::<i64>()
                .ok()
                .filter(|v| *v >= 0 && *v <= MAX_SAFE_INTEGER)
                .filter(|_| raw.chars().all(|c| c.is_ascii_digit()) && !raw.is_empty());
            bounds[slot] = Some(parsed.ok_or_else(|| {
                ApiError::bad_request(format!("{key} must be an epoch timestamp in milliseconds"))
            })?);
        }
    }
    if let (Some(from), Some(to)) = (bounds[0], bounds[1]) {
        if from > to {
            return Err(ApiError::bad_request(
                "from must be less than or equal to to",
            ));
        }
    }

    Ok(SalesSummaryFilters {
        seller,
        from: bounds[0],
        to: bounds[1],
    })
}

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[cfg(test)]
mod summary_tests {
    use super::*;

    fn pairs(raw: &[(&str, &str)]) -> Vec<(String, String)> {
        raw.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn message(pairs: &[(String, String)]) -> String {
        match parse_summary_filters(pairs) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected a rejection"),
        }
    }

    #[test]
    fn a_summary_needs_a_seller_that_is_really_an_address() {
        assert_eq!(message(&pairs(&[])), "A valid seller address is required");
        assert_eq!(
            message(&pairs(&[("seller", "vitalik.eth")])),
            "A valid seller address is required"
        );
        let filters = parse_summary_filters(&pairs(&[(
            "seller",
            "0xABCDEF0123456789012345678901234567890123",
        )]))
        .unwrap();
        assert_eq!(filters.seller, "0xabcdef0123456789012345678901234567890123");
        assert_eq!((filters.from, filters.to), (None, None));
    }

    #[test]
    fn a_bound_that_is_not_an_epoch_millisecond_names_itself() {
        for (key, value) in [
            ("from", "yesterday"),
            ("to", "-1"),
            ("from", "1.5"),
            ("to", "9007199254740992"),
            ("from", ""),
        ] {
            assert_eq!(
                message(&pairs(&[
                    ("seller", "0x1111111111111111111111111111111111111111"),
                    (key, value)
                ])),
                format!("{key} must be an epoch timestamp in milliseconds"),
                "{key}={value}"
            );
        }
    }

    #[test]
    fn an_inverted_window_is_refused_rather_than_answered_empty() {
        assert_eq!(
            message(&pairs(&[
                ("seller", "0x1111111111111111111111111111111111111111"),
                ("from", "200"),
                ("to", "100"),
            ])),
            "from must be less than or equal to to"
        );
        let equal = parse_summary_filters(&pairs(&[
            ("seller", "0x1111111111111111111111111111111111111111"),
            ("from", "100"),
            ("to", "100"),
        ]))
        .unwrap();
        assert_eq!((equal.from, equal.to), (Some(100), Some(100)));
    }

    #[test]
    fn each_bound_binds_the_placeholder_that_follows_the_seller() {
        let only_to = sales_summary_sql(&SalesSummaryFilters {
            seller: "0x1".into(),
            from: None,
            to: Some(2),
        });
        assert!(
            only_to.contains("timestamp <= $2::numeric / 1000"),
            "{only_to}"
        );
        assert!(!only_to.contains("timestamp >="), "{only_to}");

        let both = sales_summary_sql(&SalesSummaryFilters {
            seller: "0x1".into(),
            from: Some(1),
            to: Some(2),
        });
        assert!(both.contains("timestamp >= $2::numeric / 1000"), "{both}");
        assert!(both.contains("timestamp <= $3::numeric / 1000"), "{both}");
        assert_eq!(
            both.matches("timestamp >= $2::numeric / 1000").count(),
            2,
            "the window bounds the seller's sales and the royalties leg alike"
        );
    }

    #[test]
    fn the_window_never_multiplies_the_indexed_timestamp() {
        let sql = sales_summary_sql(&SalesSummaryFilters {
            seller: "0x1".into(),
            from: Some(1),
            to: Some(2),
        });
        assert!(
            !sql.contains("timestamp * 1000"),
            "multiplying the column would discard the sale timestamp index: {sql}"
        );
    }

    #[test]
    fn the_royalties_leg_matches_a_creator_in_any_case() {
        let sql = sales_summary_sql(&SalesSummaryFilters {
            seller: "0x1".into(),
            ..Default::default()
        });
        assert!(sql.contains("LOWER(i.creator) = $1"), "{sql}");
        assert!(sql.contains("s.type IN ('order', 'bid')"), "{sql}");
    }

    #[test]
    fn the_item_leg_counts_lifetime_mints_and_the_collection_leg_the_window() {
        let sql = sales_summary_sql(&SalesSummaryFilters {
            seller: "0x1".into(),
            ..Default::default()
        });
        let items = sql.split("), items AS (").nth(1).unwrap();
        assert!(items.contains("FROM seller_sales"), "{items}");
        let collections = sql.split("), collections AS (").nth(1).unwrap();
        assert!(collections.contains("FROM window_sales"), "{collections}");
    }
}
