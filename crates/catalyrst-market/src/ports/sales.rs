use serde::Serialize;
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
pub struct Sale {
    pub id: String,
    #[serde(rename = "itemId")]
    pub item_id: Option<String>,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    pub buyer: String,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub network: Network,
    pub price: String,
    pub seller: String,
    pub timestamp: i64,
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "type")]
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
            // Sort on the RAW (uncast) sort_* passthrough columns (sale.price and
            // sale.timestamp are both numeric) rather than the ::text/(*1000)
            // output aliases. A cast in the sort key blocks the sale(timestamp)
            // index and forced a full top-N sort over every matching sale;
            // ordering on the raw column lets the timestamp page early-terminate.
            // Numeric ordering is identical to the previous ::numeric/bigint cast.
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
        // network filter: upstream applies `network = ANY(getDBNetworks(network))`
        // (ETHEREUM -> [ETHEREUM]; MATIC -> [MATIC, POLYGON]). Was parsed but never
        // applied here, so ?network= leaked sales from the other chain.
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

        // Split the COUNT(*) OVER() into a page + a concurrent count, same as
        // get_orders. The window forced Postgres to materialize every matching
        // sale before returning a page; split, the page early-terminates through
        // the sale(timestamp) index. SET LOCAL hints are scoped to each
        // statement's transaction, never the shared cluster config.
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

        // Only the timestamp sort has a supporting index; gate enable_sort=off
        // on it. random_page_cost=1.1 (SSD-correct) is always safe.
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
