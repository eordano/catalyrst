use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;

use crate::dcl_schemas::{ChainId, Network};
use crate::http::errors::InvalidParameterError;
use crate::http::pagination::{get_pagination_params, get_parameter};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidSortBy {
    RecentlyOffered,
    RecentlyUpdated,
    MostExpensive,
}

#[derive(Debug, Clone, Default)]
pub struct BidFilters {
    pub limit: i64,
    pub offset: i64,
    pub bidder: Option<String>,
    pub seller: Option<String>,
    pub sort_by: Option<BidSortBy>,
    pub contract_address: Option<String>,
    pub token_id: Option<String>,
    pub item_id: Option<String>,
    pub network: Option<Network>,
    pub network_db_filter: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Bid {
    pub id: String,
    pub bidder: String,
    pub price: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub fingerprint: String,
    pub status: String,
    pub seller: String,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(rename = "tradeId", skip_serializing_if = "Option::is_none")]
    pub trade_id: Option<String>,
    #[serde(
        rename = "tradeContractAddress",
        skip_serializing_if = "Option::is_none"
    )]
    pub trade_contract_address: Option<String>,
    #[serde(rename = "bidAddress", skip_serializing_if = "Option::is_none")]
    pub bid_address: Option<String>,
    #[serde(rename = "blockchainId", skip_serializing_if = "Option::is_none")]
    pub blockchain_id: Option<String>,
    #[serde(rename = "blockNumber", skip_serializing_if = "Option::is_none")]
    pub block_number: Option<String>,
}

pub struct BidsComponent {
    pool: PgPool,
}

impl BidsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_bids(&self, f: &BidFilters) -> Result<(Vec<Bid>, i64), ApiError> {
        let order_by = match f.sort_by {
            Some(BidSortBy::RecentlyUpdated) => "updated_at DESC",
            // outer `price` alias is price::text — sort numerically (same lex bug
            // as sales most_expensive). updated_at/created_at are numeric (ok).
            Some(BidSortBy::MostExpensive) => "price::numeric DESC",
            _ => "created_at DESC",
        };

        let mut binds: Vec<String> = Vec::new();
        let mut where_parts: Vec<String> = Vec::new();
        let mut idx: usize = 0;
        let mut next = || {
            idx += 1;
            format!("${}", idx)
        };
        if let Some(ref v) = f.bidder {
            where_parts.push(format!("LOWER(bidder) = LOWER({})", next()));
            binds.push(v.clone());
        }
        if let Some(ref v) = f.seller {
            where_parts.push(format!("LOWER(seller) = LOWER({})", next()));
            binds.push(v.clone());
        }
        if let Some(ref v) = f.contract_address {
            where_parts.push(format!("contract_address = {}", next()));
            binds.push(v.to_lowercase());
        }
        if let Some(ref v) = f.token_id {
            where_parts.push(format!("LOWER(token_id) = LOWER({})", next()));
            binds.push(v.clone());
        }
        if let Some(ref v) = f.item_id {
            where_parts.push(format!("LOWER(item_id) = LOWER({})", next()));
            binds.push(v.clone());
        }
        if let Some(ref v) = f.status {
            where_parts.push(format!("status = {}", next()));
            binds.push(v.clone());
        }
        if let Some(ref nets) = f.network_db_filter {
            if nets.is_empty() {
                where_parts.push("FALSE".to_string());
            } else {
                let placeholders: Vec<String> = nets.iter().map(|_| next()).collect();
                where_parts.push(format!("network IN ({})", placeholders.join(", ")));
                for n in nets {
                    binds.push(n.clone());
                }
            }
        }

        where_parts.push("expires_at > extract(epoch from now()) * 1000".to_string());

        let where_sql = format!("WHERE {}", where_parts.join(" AND "));

        let legacy_item_id_clause = if f.item_id.is_some() { "AND FALSE" } else { "" };

        let limit_p = next();
        let offset_p = next();

        let sql = format!(
            r#"
SELECT *, COUNT(*) OVER() AS bids_count FROM (
  (
    SELECT
      NULL::text                AS trade_id,
      id::text                  AS legacy_bid_id,
      NULL::text                AS trade_contract_address,
      bid_address               AS bid_address,
      blockchain_id::text       AS blockchain_id,
      block_number::text        AS block_number,
      '0x' || encode(bidder, 'hex') AS bidder,
      (created_at * 1000)::float8 AS created_at,
      (updated_at * 1000)::float8 AS updated_at,
      expires_at::float8        AS expires_at,
      network                   AS network,
      NULL::int                 AS chain_id,
      price::text               AS price,
      token_id::text            AS token_id,
      NULL::text                AS item_id,
      nft_address               AS contract_address,
      '0x' || encode(fingerprint, 'hex') AS fingerprint,
      '0x' || encode(seller, 'hex') AS seller,
      status                    AS status
    FROM {schema}.bid
    WHERE expires_at > extract(epoch from now()) * 1000 {legacy_item_id_clause}
  )
) AS combined_bids
{where_sql}
ORDER BY {order_by}
LIMIT {limit_p} OFFSET {offset_p}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            legacy_item_id_clause = legacy_item_id_clause,
            where_sql = where_sql,
            order_by = order_by,
            limit_p = limit_p,
            offset_p = offset_p,
        );

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        for s in &binds {
            q = q.bind(s);
        }
        q = q.bind(f.limit);
        q = q.bind(f.offset);

        let rows = q.fetch_all(&self.pool).await?;
        let mut total: i64 = 0;
        let bids: Vec<Bid> = rows
            .into_iter()
            .map(|r| {
                if total == 0 {
                    if let Ok(c) = r.try_get::<i64, _>("bids_count") {
                        total = c;
                    }
                }
                row_to_bid(&r)
            })
            .collect();
        Ok((bids, total))
    }
}

fn row_to_bid(r: &sqlx::postgres::PgRow) -> Bid {
    let network_str: String = r.try_get("network").unwrap_or_default();
    let (network, chain_id) = crate::ports::orders::network_and_chain(&network_str);
    let trade_id: Option<String> = r.try_get("trade_id").unwrap_or(None);
    let legacy_id: Option<String> = r.try_get("legacy_bid_id").unwrap_or(None);
    let id = trade_id.clone().or(legacy_id.clone()).unwrap_or_default();
    Bid {
        id,
        bidder: r.try_get("bidder").unwrap_or_default(),
        price: r.try_get("price").unwrap_or_default(),
        created_at: r.try_get::<f64, _>("created_at").unwrap_or(0.0) as i64,
        updated_at: r.try_get::<f64, _>("updated_at").unwrap_or(0.0) as i64,
        fingerprint: r.try_get("fingerprint").unwrap_or_default(),
        status: r
            .try_get::<Option<String>, _>("status")
            .unwrap_or(None)
            .unwrap_or_else(|| "open".to_string()),
        seller: r.try_get("seller").unwrap_or_default(),
        network,
        chain_id,
        contract_address: r.try_get("contract_address").unwrap_or_default(),
        expires_at: r.try_get::<f64, _>("expires_at").unwrap_or(0.0) as i64,
        token_id: r.try_get::<Option<String>, _>("token_id").unwrap_or(None),
        item_id: r.try_get::<Option<String>, _>("item_id").unwrap_or(None),
        trade_id,
        trade_contract_address: r
            .try_get::<Option<String>, _>("trade_contract_address")
            .unwrap_or(None),
        bid_address: r
            .try_get::<Option<String>, _>("bid_address")
            .unwrap_or(None),
        blockchain_id: r
            .try_get::<Option<String>, _>("blockchain_id")
            .unwrap_or(None),
        block_number: r
            .try_get::<Option<String>, _>("block_number")
            .unwrap_or(None),
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<BidFilters, InvalidParameterError> {
    let pg = get_pagination_params(pairs);
    let p = Params::new(pairs);

    let sort_by = get_parameter(
        "sortBy",
        pairs,
        Some(&["recently_offered", "recently_updated", "most_expensive"]),
    )?
    .map(|s| match s.as_str() {
        "recently_updated" => BidSortBy::RecentlyUpdated,
        "most_expensive" => BidSortBy::MostExpensive,
        _ => BidSortBy::RecentlyOffered,
    });

    let network_raw = get_parameter(
        "network",
        pairs,
        Some(&[
            "ETHEREUM",
            "MATIC",
            "AVALANCHE",
            "BINANCE SMART CHAIN",
            "OPTIMISM",
            "ARBITRUM",
            "FANTOM",
        ]),
    )?;

    let network = network_raw.as_deref().and_then(|s| match s {
        "ETHEREUM" => Some(Network::Ethereum),
        "MATIC" => Some(Network::Matic),
        _ => None,
    });

    let network_db_filter = network_raw.as_deref().map(|s| match s {
        "ETHEREUM" => vec!["ETHEREUM".to_string()],
        "MATIC" => vec!["MATIC".to_string(), "POLYGON".to_string()],
        _ => Vec::new(),
    });

    let status = get_parameter("status", pairs, Some(&["open", "sold", "cancelled"]))?;

    Ok(BidFilters {
        limit: pg.limit,
        offset: pg.offset,
        bidder: p.get_string("bidder", None),
        seller: p.get_string("seller", None),
        sort_by,
        contract_address: p.get_string("contractAddress", None),
        token_id: p.get_string("tokenId", None),
        item_id: p.get_string("itemId", None),
        network,
        network_db_filter,
        status,
    })
}
