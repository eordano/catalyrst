use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;

use crate::dcl_schemas::{ethereum_chain_id, polygon_chain_id, ChainId, Network};
use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSortBy {
    Oldest,
    RecentlyListed,
    RecentlyUpdated,
    Cheapest,
    IssuedIdAsc,
    IssuedIdDesc,
}

#[derive(Debug, Clone, Default)]
pub struct OrderFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<OrderSortBy>,
    pub marketplace_address: Option<String>,
    pub owner: Option<String>,
    pub buyer: Option<String>,
    pub contract_address: Option<String>,
    pub token_id: Option<String>,
    pub status: Option<String>,
    pub network: Option<Network>,
    pub item_id: Option<String>,
    pub nft_name: Option<String>,
    pub nft_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct Order {
    pub id: String,
    #[serde(rename = "marketplaceAddress")]
    pub marketplace_address: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "tokenId")]
    pub token_id: Option<String>,
    pub owner: String,
    pub buyer: Option<String>,
    pub price: String,
    pub status: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: f64,
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "updatedAt")]
    pub updated_at: f64,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    #[serde(rename = "issuedId")]
    pub issued_id: Option<String>,
    #[serde(rename = "tradeId")]
    pub trade_id: Option<String>,
}

pub struct OrdersComponent {
    pool: PgPool,
}

impl OrdersComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_orders(&self, filters: &OrderFilters) -> Result<(Vec<Order>, i64), ApiError> {
        let limit = filters.first.unwrap_or(1000);
        let offset = filters.skip.unwrap_or(0);

        let order_by = match filters.sort_by {
            Some(OrderSortBy::Oldest) => "created_at ASC",
            Some(OrderSortBy::RecentlyUpdated) => "updated_at DESC",
            Some(OrderSortBy::Cheapest) => "price ASC",
            Some(OrderSortBy::IssuedIdAsc) => "token_id ASC",
            Some(OrderSortBy::IssuedIdDesc) => "token_id DESC",
            _ => "created_at DESC",
        };

        let mut where_parts: Vec<String> = Vec::new();
        let mut bind_strings: Vec<String> = Vec::new();
        let mut bind_idx: usize = 0;
        let mut next_param = || {
            bind_idx += 1;
            format!("${}", bind_idx)
        };

        if let Some(ref v) = filters.marketplace_address {
            where_parts.push(format!(
                "LOWER(marketplace_address) = LOWER({})",
                next_param()
            ));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.owner {
            where_parts.push(format!("LOWER(owner) = LOWER({})", next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.buyer {
            where_parts.push(format!("LOWER(buyer) = LOWER({})", next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.contract_address {
            where_parts.push(format!("LOWER(nft_address) = LOWER({})", next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.status {
            where_parts.push(format!("status = {}", next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.item_id {
            where_parts.push(format!("item_id = {}", next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.token_id {
            where_parts.push(format!("token_id = {}", next_param()));
            bind_strings.push(v.clone());
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        let limit_param = next_param();
        let offset_param = next_param();

        // NOTE: created_at, updated_at, expires_at on squid_marketplace.order are
        // numeric (epoch seconds, written by the indexer), not timestamps. The
        // upstream marketplace-server uses EXTRACT(EPOCH FROM ...) because it
        // unions in marketplace.trades (where they are timestamptz). Here we keep
        // them as-is for the order table and skip the trades branch (the trades
        // CTE — `unified_trades` — is provided by marketplace.mv_trades which we
        // do not have locally; see items/trades stub for context).
        let sql = format!(
            r#"
SELECT
  combined_orders.*, COUNT(*) OVER() AS count
FROM (
  SELECT
    ord.id::text                  AS id,
    ''                            AS trade_id,
    ord.marketplace_address       AS marketplace_address,
    ord.category                  AS category,
    ord.nft_address               AS nft_address,
    ord.token_id::text            AS token_id,
    ord.price::text               AS price,
    ord.item_id                   AS item_id,
    nft.issued_id::text           AS issued_id,
    ord.nft_id                    AS nft_id,
    nft.name                      AS nft_name,
    ord.owner                     AS owner,
    ord.buyer                     AS buyer,
    ord.tx_hash                   AS tx_hash,
    ord.block_number              AS block_number,
    ord.status                    AS status,
    ord.created_at::float8        AS created_at,
    ord.updated_at::float8        AS updated_at,
    ord.expires_at::float8        AS expires_at,
    ord.network                   AS network
  FROM {schema}."order" ord
  JOIN {schema}."nft" nft ON ord.nft_id = nft.id AND nft.owner_address = ord.owner
  WHERE ord.expires_at_normalized > NOW()
) AS combined_orders
{where_clause}
ORDER BY {order_by}
LIMIT {limit_param} OFFSET {offset_param}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_clause = where_clause,
            order_by = order_by,
            limit_param = limit_param,
            offset_param = offset_param,
        );

        let mut q = sqlx::query(&sql);
        for s in &bind_strings {
            q = q.bind(s);
        }
        q = q.bind(limit);
        q = q.bind(offset);

        let rows = q.fetch_all(&self.pool).await?;
        let mut total: i64 = 0;
        let orders: Vec<Order> = rows
            .into_iter()
            .map(|r| {
                if total == 0 {
                    if let Ok(c) = r.try_get::<i64, _>("count") {
                        total = c;
                    }
                }
                row_to_order(&r)
            })
            .collect();
        Ok((orders, total))
    }
}

fn row_to_order(r: &sqlx::postgres::PgRow) -> Order {
    let network_str: String = r.try_get("network").unwrap_or_default();
    let (network, chain_id) = network_and_chain(&network_str);
    Order {
        id: r.try_get("id").unwrap_or_default(),
        marketplace_address: r.try_get("marketplace_address").unwrap_or_default(),
        contract_address: r.try_get("nft_address").unwrap_or_default(),
        token_id: r.try_get::<Option<String>, _>("token_id").unwrap_or(None),
        owner: r.try_get("owner").unwrap_or_default(),
        buyer: r.try_get::<Option<String>, _>("buyer").unwrap_or(None),
        price: r.try_get("price").unwrap_or_default(),
        status: r.try_get("status").unwrap_or_default(),
        expires_at: r.try_get::<f64, _>("expires_at").unwrap_or(0.0),
        created_at: r
            .try_get::<f64, _>("created_at")
            .map(|v| v * 1000.0)
            .unwrap_or(0.0),
        updated_at: r
            .try_get::<f64, _>("updated_at")
            .map(|v| v * 1000.0)
            .unwrap_or(0.0),
        network,
        chain_id,
        issued_id: r.try_get::<Option<String>, _>("issued_id").unwrap_or(None),
        trade_id: r.try_get::<Option<String>, _>("trade_id").unwrap_or(None),
    }
}

pub(crate) fn network_and_chain(db: &str) -> (Network, ChainId) {
    match db {
        "ETHEREUM" => (Network::Ethereum, ethereum_chain_id()),
        "MATIC" | "POLYGON" => (Network::Matic, polygon_chain_id()),
        _ => (Network::Ethereum, ethereum_chain_id()),
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<OrderFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    let sort_by = p
        .get_value(
            "sortBy",
            &[
                "oldest",
                "recently_listed",
                "recently_updated",
                "cheapest",
                "issued_id_asc",
                "issued_id_desc",
            ],
            None,
        )
        .map(|s| match s.as_str() {
            "oldest" => OrderSortBy::Oldest,
            "recently_listed" => OrderSortBy::RecentlyListed,
            "recently_updated" => OrderSortBy::RecentlyUpdated,
            "cheapest" => OrderSortBy::Cheapest,
            "issued_id_asc" => OrderSortBy::IssuedIdAsc,
            "issued_id_desc" => OrderSortBy::IssuedIdDesc,
            _ => OrderSortBy::RecentlyListed,
        });

    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            _ => Network::Matic,
        });

    Ok(OrderFilters {
        first: p.get_number("first", None).map(|v| v as i64),
        skip: p.get_number("skip", None).map(|v| v as i64),
        sort_by,
        marketplace_address: p.get_string("marketplaceAddress", None),
        owner: p.get_string("owner", None),
        buyer: p.get_string("buyer", None),
        contract_address: p.get_string("contractAddress", None),
        token_id: p.get_string("tokenId", None),
        status: p.get_string("status", None),
        network,
        item_id: p.get_string("itemId", None),
        nft_name: p.get_string("nftName", None),
        nft_ids: None,
    })
}
