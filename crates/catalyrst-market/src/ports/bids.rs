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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Bid {
    pub id: String,
    pub bidder: String,
    pub price: String,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    pub fingerprint: String,
    pub status: String,
    pub seller: String,
    pub network: Network,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub chain_id: ChainId,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "expiresAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub item_id: Option<String>,
    #[serde(rename = "tradeId", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub trade_id: Option<String>,
    #[serde(
        rename = "tradeContractAddress",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub trade_contract_address: Option<String>,
    #[serde(rename = "bidAddress", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub bid_address: Option<String>,
    #[serde(rename = "blockchainId", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub blockchain_id: Option<String>,
    #[serde(rename = "blockNumber", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
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

        let sql = build_combined_bids_sql(
            &where_sql,
            order_by,
            &limit_p,
            &offset_p,
            legacy_item_id_clause,
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

fn bid_trades_source() -> String {
    format!(
        r#"
    SELECT
      t.id,
      t.contract                  AS trade_contract_address,
      t.created_at,
      t.signer,
      t.expires_at,
      t.checks,
      t.network,
      t.chain_id,
      json_object_agg(av.direction, json_build_object(
        'contract_address', av.contract_address,
        'direction',        av.direction,
        'beneficiary',      av.beneficiary,
        'extra',            av.extra,
        'token_id',         av.token_id,
        'item_id',          av.item_id,
        'amount',           av.amount,
        'creator',          av.creator,
        'owner',            av.owner,
        'category',         av.category,
        'nft_id',           av.nft_id,
        'issued_id',        av.issued_id,
        'nft_name',         av.nft_name
      )) AS assets,
      CASE
        WHEN exec.executions >= (t.checks ->> 'uses')::int THEN 'sold'
        WHEN canc.cancellations > 0                        THEN 'cancelled'
        WHEN t.expires_at < now()::timestamptz(3)          THEN 'cancelled'
        ELSE 'open'
      END AS status
    FROM marketplace.trades AS t
    JOIN (
      SELECT
        ta.trade_id,
        ta.contract_address,
        ta.direction::text AS direction,
        ta.beneficiary,
        ta.extra,
        erc721_asset.token_id,
        coalesce(item_asset.item_id, nft.item_blockchain_id::text) AS item_id,
        erc20_asset.amount,
        item.creator,
        nft.owner_address AS owner,
        nft.category,
        nft.id            AS nft_id,
        nft.issued_id     AS issued_id,
        nft.name          AS nft_name
      FROM marketplace.trade_assets AS ta
      LEFT JOIN marketplace.trade_assets_erc721 AS erc721_asset ON ta.id = erc721_asset.asset_id
      LEFT JOIN marketplace.trade_assets_erc20  AS erc20_asset  ON ta.id = erc20_asset.asset_id
      LEFT JOIN marketplace.trade_assets_item   AS item_asset   ON ta.id = item_asset.asset_id
      LEFT JOIN {schema}.item AS item
        ON (ta.contract_address = item.collection_id AND item_asset.item_id::numeric = item.blockchain_id)
      LEFT JOIN {schema}.nft AS nft
        ON (ta.contract_address = nft.contract_address AND erc721_asset.token_id::numeric = nft.token_id)
    ) AS av ON t.id = av.trade_id
    LEFT JOIN (
      SELECT order_signature_hash AS hashed_signature, COUNT(*) AS executions
      FROM marketplace.market_trades_local GROUP BY order_signature_hash
    ) AS exec ON exec.hashed_signature = t.hashed_signature
    LEFT JOIN (
      SELECT target_signature_hash AS hashed_signature, COUNT(*) AS cancellations
      FROM marketplace.market_cancellations GROUP BY target_signature_hash
    ) AS canc ON canc.hashed_signature = t.hashed_signature
    WHERE t.type = 'bid'
    GROUP BY t.id, t.contract, t.created_at, t.network, t.chain_id, t.signer,
             t.checks, t.expires_at, exec.executions, canc.cancellations
"#,
        schema = MARKETPLACE_SQUID_SCHEMA,
    )
}

fn bid_trades_branch() -> String {
    format!(
        r#"
    SELECT
      trades.id::text             AS trade_id,
      NULL::text                  AS legacy_bid_id,
      trades.trade_contract_address AS trade_contract_address,
      NULL::text                  AS bid_address,
      NULL::text                  AS blockchain_id,
      NULL::text                  AS block_number,
      trades.signer               AS bidder,
      (EXTRACT(EPOCH FROM trades.created_at) * 1000)::float8 AS created_at,
      (EXTRACT(EPOCH FROM trades.created_at) * 1000)::float8 AS updated_at,
      (EXTRACT(EPOCH FROM trades.expires_at) * 1000)::float8 AS expires_at,
      trades.network              AS network,
      trades.chain_id             AS chain_id,
      (trades.assets -> 'sent' ->> 'amount')::numeric(78)::text AS price,
      trades.assets -> 'received' ->> 'token_id'         AS token_id,
      trades.assets -> 'received' ->> 'item_id'          AS item_id,
      trades.assets -> 'received' ->> 'contract_address' AS contract_address,
      trades.assets -> 'received' ->> 'extra'            AS fingerprint,
      COALESCE(trades.assets -> 'received' ->> 'creator', trades.assets -> 'received' ->> 'owner') AS seller,
      trades.status               AS status
    FROM ({source}) AS trades
"#,
        source = bid_trades_source(),
    )
}

fn legacy_bids_branch(legacy_item_id_clause: &str) -> String {
    format!(
        r#"
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
"#,
        schema = MARKETPLACE_SQUID_SCHEMA,
        legacy_item_id_clause = legacy_item_id_clause,
    )
}

pub(crate) fn build_combined_bids_sql(
    where_sql: &str,
    order_by: &str,
    limit_p: &str,
    offset_p: &str,
    legacy_item_id_clause: &str,
) -> String {
    format!(
        "SELECT *, COUNT(*) OVER() AS bids_count FROM ( ({trades}) UNION ALL ({legacy}) ) AS combined_bids {where_sql} ORDER BY {order_by} LIMIT {limit_p} OFFSET {offset_p}",
        trades = bid_trades_branch(),
        legacy = legacy_bids_branch(legacy_item_id_clause),
    )
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

#[cfg(test)]
mod query_tests {
    use super::build_combined_bids_sql;

    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn unions_offchain_bid_trades_with_legacy_bids() {
        let sql = build_combined_bids_sql(
            " WHERE LOWER(bidder) = LOWER($1)",
            "created_at DESC",
            "$2",
            "$3",
            "",
        );

        assert!(
            sql.contains("UNION ALL"),
            "must UNION the two branches: {sql}"
        );
        assert!(
            sql.contains("WHERE t.type = 'bid'"),
            "off-chain bid_trades branch must select bid trades: {sql}"
        );
        assert!(
            sql.contains("FROM marketplace.trades AS t"),
            "off-chain bids come from marketplace.trades: {sql}"
        );
        assert!(sql.contains("marketplace.market_trades_local"), "{sql}");
        assert!(sql.contains("marketplace.market_cancellations"), "{sql}");
        assert!(
            sql.contains("FROM squid_marketplace.bid"),
            "legacy bid table must remain: {sql}"
        );
        assert!(sql.contains("COUNT(*) OVER() AS bids_count"), "{sql}");
        assert!(sql.contains("AS combined_bids"), "{sql}");
        assert!(
            sql.contains(" WHERE LOWER(bidder) = LOWER($1)"),
            "outer filter applied: {sql}"
        );
        assert!(
            sql.contains("ORDER BY created_at DESC LIMIT $2 OFFSET $3"),
            "outer sort + paginate: {sql}"
        );
    }

    #[test]
    fn item_id_path_excludes_legacy_via_false() {
        let with_item = build_combined_bids_sql("", "created_at DESC", "$1", "$2", "AND FALSE");
        assert!(
            with_item.contains("AND FALSE"),
            "legacy excluded on itemId path: {with_item}"
        );

        let without_item = build_combined_bids_sql("", "created_at DESC", "$1", "$2", "");
        assert!(
            !without_item.contains("AND FALSE"),
            "no exclusion without itemId: {without_item}"
        );
        assert!(with_item.contains("WHERE t.type = 'bid'"), "{with_item}");
    }

    #[test]
    fn both_branches_expose_the_same_response_columns() {
        let sql = build_combined_bids_sql("", "created_at DESC", "$1", "$2", "");
        for col in [
            "AS trade_id",
            "AS legacy_bid_id",
            "AS bid_address",
            "AS blockchain_id",
            "AS block_number",
            "AS bidder",
            "AS created_at",
            "AS updated_at",
            "AS expires_at",
            "AS network",
            "AS chain_id",
            "AS price",
            "AS token_id",
            "AS contract_address",
            "AS fingerprint",
            "AS seller",
        ] {
            assert_eq!(
                count_occurrences(&sql, col),
                2,
                "column `{col}` must be projected by BOTH the bid_trades and legacy branch"
            );
        }
    }
}
