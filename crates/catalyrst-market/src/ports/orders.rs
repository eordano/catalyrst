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
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
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
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    pub network: Network,
    #[serde(rename = "chainId")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
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
        let limit = crate::logic::sql_filters::clamp_first(filters.first, 1000);
        let offset = crate::logic::sql_filters::clamp_skip(filters.skip);

        let order_by = match filters.sort_by {
            Some(OrderSortBy::Oldest) => "sort_created_at ASC, sort_id ASC",
            Some(OrderSortBy::RecentlyUpdated) => "sort_updated_at DESC, sort_id ASC",
            Some(OrderSortBy::Cheapest) => "sort_price ASC, sort_id ASC",
            Some(OrderSortBy::IssuedIdAsc) => "sort_token_id ASC, sort_id ASC",
            Some(OrderSortBy::IssuedIdDesc) => "sort_token_id DESC, sort_id ASC",
            _ => "sort_created_at DESC, sort_id ASC",
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
            where_parts.push(format!("owner = {}", next_param()));
            bind_strings.push(v.to_lowercase());
        }
        if let Some(ref v) = filters.buyer {
            where_parts.push(format!("buyer = {}", next_param()));
            bind_strings.push(v.to_lowercase());
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
            where_parts.push(item_id_predicate_sql(&next_param()));
            bind_strings.push(v.clone());
        }
        if let Some(ref v) = filters.token_id {
            where_parts.push(format!("token_id = {}", next_param()));
            bind_strings.push(v.clone());
        }

        if let Some(net) = filters.network {
            let db_nets: &[&str] = match net {
                Network::Ethereum => &["ETHEREUM"],
                Network::Matic => &["MATIC", "POLYGON"],
            };
            let placeholders: Vec<String> = db_nets.iter().map(|_| next_param()).collect();
            where_parts.push(format!("network IN ({})", placeholders.join(", ")));
            for n in db_nets {
                bind_strings.push((*n).to_string());
            }
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        let limit_param = next_param();
        let offset_param = next_param();

        let page_sql =
            build_combined_orders_page_sql(&where_clause, order_by, &limit_param, &offset_param);
        let count_sql = build_combined_orders_count_sql(&where_clause);

        let page_binds = bind_strings.clone();
        let count_binds = bind_strings.clone();
        let page_pool = self.pool.clone();
        let count_pool = self.pool.clone();

        let page_fut = async move {
            let mut tx = page_pool.begin().await?;
            sqlx::query("SET LOCAL random_page_cost = 1.1")
                .execute(&mut *tx)
                .await?;
            let mut q = sqlx::query(sqlx::AssertSqlSafe(page_sql));
            for s in &page_binds {
                q = q.bind(s);
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
            for s in &count_binds {
                q = q.bind(s);
            }
            let row = q.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>(row.try_get::<i64, _>("count").unwrap_or(0))
        };

        let (rows, total) = tokio::try_join!(page_fut, count_fut)?;
        let orders: Vec<Order> = rows.iter().map(row_to_order).collect();
        Ok((orders, total))
    }
}

pub(crate) fn item_id_predicate_sql(param: &str) -> String {
    format!(
        "(item_id = {param} OR (item_id = NULLIF(split_part({param}, '-', 2), '') \
         AND LOWER(nft_address) = LOWER(split_part({param}, '-', 1))) \
         OR item_id IN (SELECT id FROM {schema}.item WHERE (collection_id || '-' || blockchain_id::text) = LOWER({param})))",
        schema = MARKETPLACE_SQUID_SCHEMA,
    )
}

fn orders_trades_cte() -> &'static str {
    " WITH unified_trades AS ( SELECT * FROM marketplace.mv_trades ) "
}

fn orders_trades_branch() -> &'static str {
    r#"
  SELECT
    trades.id::text                                              AS id,
    trades.id::text                                              AS trade_id,
    trades.trade_contract                                        AS marketplace_address,
    trades.sent_nft_category::text                              AS category,
    trades.contract_address_sent                                AS nft_address,
    (trades.sent_token_id)::numeric(78)::text                   AS token_id,
    (trades.amount_received)::numeric(78)::text                 AS price,
    trades.sent_item_id::text                                   AS item_id,
    (trades.assets -> 'sent' ->> 'issued_id')::numeric(78)::text AS issued_id,
    trades.assets -> 'sent' ->> 'nft_id'                        AS nft_id,
    trades.assets -> 'sent' ->> 'nft_name'                      AS nft_name,
    trades.assets -> 'sent' ->> 'owner'                         AS owner,
    ''                                                          AS buyer,
    ''                                                          AS tx_hash,
    0                                                          AS block_number,
    trades.status::text                                        AS status,
    EXTRACT(EPOCH FROM trades.created_at)::float8               AS created_at,
    EXTRACT(EPOCH FROM trades.created_at)::float8               AS updated_at,
    EXTRACT(EPOCH FROM trades.expires_at)::float8              AS expires_at,
    trades.network::text                                       AS network,
    EXTRACT(EPOCH FROM trades.created_at)                       AS sort_created_at,
    EXTRACT(EPOCH FROM trades.created_at)                       AS sort_updated_at,
    (trades.amount_received)::numeric(78)                       AS sort_price,
    (trades.sent_token_id)::numeric(78)                         AS sort_token_id,
    trades.id::text                                            AS sort_id
  FROM (
    SELECT * FROM unified_trades WHERE type = 'public_nft_order' AND status = 'open'
  ) AS trades
  WHERE trades.signer = trades.assets -> 'sent' ->> 'owner'
"#
}

fn orders_legacy_branch() -> String {
    format!(
        r#"
  SELECT
    ord.id::text                  AS id,
    ''                            AS trade_id,
    ord.marketplace_address       AS marketplace_address,
    ord.category::text            AS category,
    ord.nft_address               AS nft_address,
    ord.token_id::text            AS token_id,
    ord.price::text               AS price,
    ord.item_id::text             AS item_id,
    nft.issued_id::text           AS issued_id,
    ord.nft_id                    AS nft_id,
    nft.name                      AS nft_name,
    ord.owner                     AS owner,
    ord.buyer                     AS buyer,
    ord.tx_hash                   AS tx_hash,
    ord.block_number              AS block_number,
    ord.status::text              AS status,
    ord.created_at::float8        AS created_at,
    ord.updated_at::float8        AS updated_at,
    ord.expires_at::float8        AS expires_at,
    ord.network::text             AS network,
    ord.created_at                AS sort_created_at,
    ord.updated_at                AS sort_updated_at,
    ord.price                     AS sort_price,
    ord.token_id                  AS sort_token_id,
    ord.id::text                  AS sort_id
  FROM {schema}."order" ord
  JOIN {schema}."nft" nft ON ord.nft_id = nft.id AND nft.owner_address = ord.owner
  WHERE ord.expires_at_normalized > NOW()
"#,
        schema = MARKETPLACE_SQUID_SCHEMA,
    )
}

pub(crate) fn build_open_orders_by_nft_ids_sql(with_owner: bool) -> String {
    let owner_clause = if with_owner {
        " AND LOWER(combined_orders.owner) = LOWER($2)"
    } else {
        ""
    };
    format!(
        "{cte}SELECT combined_orders.* FROM ( ({trades}) UNION ALL ({legacy}) ) AS combined_orders WHERE combined_orders.status = 'open' AND combined_orders.nft_id = ANY($1){owner_clause} ORDER BY sort_created_at DESC, sort_id ASC",
        cte = orders_trades_cte(),
        trades = orders_trades_branch(),
        legacy = orders_legacy_branch(),
    )
}

pub(crate) fn build_combined_orders_page_sql(
    where_clause: &str,
    order_by: &str,
    limit_param: &str,
    offset_param: &str,
) -> String {
    format!(
        "{cte}SELECT combined_orders.* FROM ( ({trades}) UNION ALL ({legacy}) ) AS combined_orders{where_clause} ORDER BY {order_by} LIMIT {limit_param} OFFSET {offset_param}",
        cte = orders_trades_cte(),
        trades = orders_trades_branch(),
        legacy = orders_legacy_branch(),
    )
}

pub(crate) fn build_combined_orders_count_sql(where_clause: &str) -> String {
    format!(
        "{cte}SELECT COUNT(*)::int8 AS count FROM ( ({trades}) UNION ALL ({legacy}) ) AS combined_orders{where_clause}",
        cte = orders_trades_cte(),
        trades = orders_trades_branch(),
        legacy = orders_legacy_branch(),
    )
}

pub(crate) fn row_to_order(r: &sqlx::postgres::PgRow) -> Order {
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

        expires_at: r.try_get::<f64, _>("expires_at").unwrap_or(0.0) as i64,

        created_at: from_seconds_to_milliseconds(r.try_get::<f64, _>("created_at").unwrap_or(0.0)),
        updated_at: from_seconds_to_milliseconds(r.try_get::<f64, _>("updated_at").unwrap_or(0.0)),
        network,
        chain_id,
        issued_id: r.try_get::<Option<String>, _>("issued_id").unwrap_or(None),
        trade_id: r.try_get::<Option<String>, _>("trade_id").unwrap_or(None),
    }
}

pub(crate) fn from_seconds_to_milliseconds(time: f64) -> i64 {
    if time < 1e11_f64 {
        (time * 1000.0).round() as i64
    } else {
        time.round() as i64
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

#[cfg(test)]
mod ms_tests {
    use super::from_seconds_to_milliseconds;

    #[test]
    fn seconds_are_scaled_to_milliseconds() {
        assert_eq!(
            from_seconds_to_milliseconds(1_700_000_000.0),
            1_700_000_000_000
        );

        assert_eq!(
            from_seconds_to_milliseconds(1_000_000_000.0),
            1_000_000_000_000
        );

        assert_eq!(from_seconds_to_milliseconds(0.0), 0);
    }

    #[test]
    fn millisecond_values_pass_through() {
        assert_eq!(
            from_seconds_to_milliseconds(1_700_000_000_000.0),
            1_700_000_000_000
        );

        assert_eq!(from_seconds_to_milliseconds(1e11_f64), 100_000_000_000);
    }
}

#[cfg(test)]
mod query_tests {
    use super::{
        build_combined_orders_count_sql, build_combined_orders_page_sql, item_id_predicate_sql,
    };

    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn page_query_unions_offchain_trades_with_legacy_orders() {
        let sql = build_combined_orders_page_sql(
            " WHERE owner = $1",
            "sort_created_at DESC, sort_id ASC",
            "$2",
            "$3",
        );

        assert!(
            sql.contains("UNION ALL"),
            "must UNION the two branches: {sql}"
        );
        assert!(
            sql.contains("WITH unified_trades AS ( SELECT * FROM marketplace.mv_trades )"),
            "off-chain trades CTE must be present: {sql}"
        );
        assert!(
            sql.contains("WHERE type = 'public_nft_order' AND status = 'open'"),
            "trades branch must select public_nft_order open listings: {sql}"
        );
        assert!(
            sql.contains(r#"squid_marketplace."order" ord"#),
            "legacy order table must remain: {sql}"
        );
        assert!(sql.contains("AS combined_orders"), "{sql}");
        assert!(
            sql.contains(" WHERE owner = $1"),
            "outer filter applied: {sql}"
        );
        assert!(
            sql.contains("ORDER BY sort_created_at DESC, sort_id ASC LIMIT $2 OFFSET $3"),
            "outer sort + paginate: {sql}"
        );
    }

    #[test]
    fn both_branches_expose_the_same_response_columns() {
        let sql =
            build_combined_orders_page_sql("", "sort_created_at DESC, sort_id ASC", "$1", "$2");
        for col in [
            "AS trade_id",
            "AS marketplace_address",
            "AS nft_address",
            "AS token_id",
            "AS price",
            "AS item_id",
            "AS issued_id",
            "AS owner",
            "AS buyer",
            "AS status",
            "AS created_at",
            "AS updated_at",
            "AS expires_at",
            "AS network",
            "AS sort_price",
            "AS sort_id",
        ] {
            assert_eq!(
                count_occurrences(&sql, col),
                2,
                "column `{col}` must be projected by BOTH the trades and legacy branch"
            );
        }
    }

    #[test]
    fn item_id_filter_matches_composite_and_plain_forms() {
        let sql = item_id_predicate_sql("$1");

        assert!(
            sql.contains("item_id = $1"),
            "composite form must still match exactly: {sql}"
        );
        assert!(
            sql.contains("item_id = NULLIF(split_part($1, '-', 2), '')"),
            "plain form must match the id suffix of the composite param: {sql}"
        );
        assert!(
            sql.contains("LOWER(nft_address) = LOWER(split_part($1, '-', 1))"),
            "plain-form match must be scoped to the contract (plain ids are only unique per collection): {sql}"
        );
        assert!(
            sql.contains(" OR "),
            "the two forms are alternatives over the UNION branches: {sql}"
        );
        assert!(
            !sql.contains("$2"),
            "predicate must reuse the one bind: {sql}"
        );
    }

    #[test]
    fn item_id_filter_is_embedded_in_page_and_count_sql() {
        let clause = format!(" WHERE {}", item_id_predicate_sql("$1"));
        let page = build_combined_orders_page_sql(
            &clause,
            "sort_created_at DESC, sort_id ASC",
            "$2",
            "$3",
        );
        let count = build_combined_orders_count_sql(&clause);
        for sql in [&page, &count] {
            assert!(
                sql.contains("NULLIF(split_part($1, '-', 2), '')"),
                "combined query must carry the dual-form item filter: {sql}"
            );
        }
    }

    #[test]
    fn open_orders_by_nft_ids_spans_trades_and_legacy() {
        let sql = super::build_open_orders_by_nft_ids_sql(false);
        assert!(sql.contains("UNION ALL"), "must span both branches: {sql}");
        assert!(
            sql.contains("WHERE type = 'public_nft_order' AND status = 'open'"),
            "trade listings (e.g. ENS names listed off-chain) must surface as orders: {sql}"
        );
        assert!(sql.contains(r#"squid_marketplace."order" ord"#), "{sql}");
        assert!(
            sql.contains("combined_orders.status = 'open' AND combined_orders.nft_id = ANY($1)"),
            "{sql}"
        );
        assert!(
            sql.contains("ORDER BY sort_created_at DESC, sort_id ASC"),
            "most recent listing must win per nft: {sql}"
        );
        assert!(!sql.contains("$2"), "{sql}");

        let with_owner = super::build_open_orders_by_nft_ids_sql(true);
        assert!(
            with_owner.contains("LOWER(combined_orders.owner) = LOWER($2)"),
            "{with_owner}"
        );
    }

    #[test]
    fn count_query_covers_the_same_union() {
        let sql = build_combined_orders_count_sql(" WHERE status = $1");
        assert!(sql.contains("COUNT(*)::int8 AS count"), "{sql}");
        assert!(
            sql.contains("UNION ALL"),
            "count must span both branches: {sql}"
        );
        assert!(
            sql.contains("WHERE type = 'public_nft_order'"),
            "count must include off-chain trades: {sql}"
        );
        assert!(
            sql.contains(r#"squid_marketplace."order" ord"#),
            "count must include legacy orders: {sql}"
        );
        assert!(
            sql.contains(" WHERE status = $1"),
            "count applies the same filter: {sql}"
        );
        assert!(!sql.contains("LIMIT"), "count must not paginate: {sql}");
    }
}
