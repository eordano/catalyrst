use serde::Serialize;
use sqlx::PgPool;

use crate::dcl_schemas::{ethereum_chain_id, get_db_networks, polygon_chain_id, ChainId, Network};
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy)]
pub enum CollectionSortBy {
    Newest,
    RecentlyReviewed,
    Name,
    Size,
    RecentlyListed,
}

#[derive(Debug, Clone, Default)]
pub struct CollectionFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<CollectionSortBy>,
    pub name: Option<String>,
    pub search: Option<String>,
    pub creator: Option<String>,
    pub urn: Option<String>,
    pub contract_address: Option<String>,
    pub is_on_sale: bool,
    pub network: Option<Network>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub urn: String,
    pub creator: String,
    pub name: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "reviewedAt")]
    pub reviewed_at: i64,
    #[serde(rename = "isOnSale")]
    pub is_on_sale: bool,
    pub size: i32,
    pub network: Network,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    #[serde(rename = "firstListedAt")]
    pub first_listed_at: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct DbCollection {
    id: String,
    creator: String,
    name: String,
    urn: String,
    items_count: i32,
    created_at: i64,
    updated_at: i64,
    reviewed_at: i64,
    first_listed_at: Option<i64>,
    search_is_store_minter: bool,
    network: String,
}

pub struct CollectionsComponent {
    pool: PgPool,
}

impl CollectionsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_collections(
        &self,
        filters: &CollectionFilters,
    ) -> Result<(Vec<Collection>, i64), ApiError> {
        const MAX_LIMIT: i64 = 1000;
        // Clamp non-negative: a negative first/skip → negative SQL LIMIT/OFFSET → 500.
        let limit = filters
            .first
            .map(|f| f.clamp(0, MAX_LIMIT))
            .unwrap_or(MAX_LIMIT);
        let offset = filters.skip.unwrap_or(0).max(0);

        let mut where_clauses: Vec<String> = vec!["is_approved = true".to_string()];
        let mut bind_idx: usize = 0;

        let contract_address_lower = filters.contract_address.as_ref().map(|s| s.to_lowercase());
        if contract_address_lower.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("id = ${}", bind_idx));
        }

        let creator_lower = filters.creator.as_ref().map(|s| s.to_lowercase());
        if creator_lower.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("creator = ${}", bind_idx));
        }

        if filters.urn.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("urn = ${}", bind_idx));
        }

        if filters.is_on_sale {
            where_clauses.push("search_is_store_minter = true".to_string());
        }

        if filters.name.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("name = ${}", bind_idx));
        }

        let search_pattern = filters
            .search
            .as_ref()
            .map(|s| format!("%{}%", s.trim().to_lowercase()));
        if search_pattern.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("search_text LIKE ${}", bind_idx));
        }

        let networks: Option<Vec<String>> = filters
            .network
            .map(|n| get_db_networks(n).into_iter().map(String::from).collect());
        if networks.is_some() {
            bind_idx += 1;
            where_clauses.push(format!("network = ANY(${}::text[])", bind_idx));
        }

        if matches!(filters.sort_by, Some(CollectionSortBy::RecentlyListed)) {
            where_clauses.push("first_listed_at IS NOT NULL".to_string());
        }

        let where_sql = format!(" WHERE {}", where_clauses.join(" AND "));

        let sort_clause = match filters.sort_by {
            Some(CollectionSortBy::Newest) => " ORDER BY created_at DESC ",
            Some(CollectionSortBy::RecentlyReviewed) => " ORDER BY reviewed_at DESC ",
            Some(CollectionSortBy::Name) => " ORDER BY name COLLATE \"C\" ASC ",
            Some(CollectionSortBy::Size) => " ORDER BY items_count DESC ",
            Some(CollectionSortBy::RecentlyListed) => " ORDER BY first_listed_at DESC ",
            None => " ORDER BY name COLLATE \"C\" ASC ",
        };

        let limit_idx = bind_idx + 1;
        let offset_idx = bind_idx + 2;

        let select_sql = format!(
            "SELECT id, creator, name, urn, items_count, \
                    created_at::int8 AS created_at, updated_at::int8 AS updated_at, \
                    reviewed_at::int8 AS reviewed_at, first_listed_at::int8 AS first_listed_at, \
                    search_is_store_minter, network \
             FROM {schema}.collection {where_} {sort_} LIMIT ${limit_idx} OFFSET ${offset_idx}",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = where_sql,
            sort_ = sort_clause,
            limit_idx = limit_idx,
            offset_idx = offset_idx,
        );

        let mut q = sqlx::query_as::<_, DbCollection>(sqlx::AssertSqlSafe(select_sql));
        if let Some(ref s) = contract_address_lower {
            q = q.bind(s);
        }
        if let Some(ref s) = creator_lower {
            q = q.bind(s);
        }
        if let Some(ref s) = filters.urn {
            q = q.bind(s);
        }
        if let Some(ref s) = filters.name {
            q = q.bind(s);
        }
        if let Some(ref s) = search_pattern {
            q = q.bind(s);
        }
        if let Some(ref n) = networks {
            q = q.bind(n);
        }
        q = q.bind(limit).bind(offset);
        let rows = q.fetch_all(&self.pool).await?;

        let count_sql = format!(
            "SELECT COUNT(*) FROM {schema}.collection {where_}",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = where_sql,
        );
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        if let Some(ref s) = contract_address_lower {
            cq = cq.bind(s);
        }
        if let Some(ref s) = creator_lower {
            cq = cq.bind(s);
        }
        if let Some(ref s) = filters.urn {
            cq = cq.bind(s);
        }
        if let Some(ref s) = filters.name {
            cq = cq.bind(s);
        }
        if let Some(ref s) = search_pattern {
            cq = cq.bind(s);
        }
        if let Some(ref n) = networks {
            cq = cq.bind(n);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let data = rows
            .into_iter()
            .map(from_db_collection_to_collection)
            .collect();
        Ok((data, total))
    }
}

fn from_seconds_to_milliseconds(time: i64) -> i64 {
    if time.abs().to_string().len() <= 10 {
        time.saturating_mul(1000)
    } else {
        time
    }
}

fn from_db_collection_to_collection(c: DbCollection) -> Collection {
    let is_polygon = matches!(c.network.as_str(), "POLYGON" | "MATIC");
    let network = if is_polygon {
        Network::Matic
    } else {
        Network::Ethereum
    };
    let chain_id = if is_polygon {
        polygon_chain_id()
    } else {
        ethereum_chain_id()
    };
    Collection {
        urn: c.urn,
        creator: c.creator,
        name: c.name,
        contract_address: c.id,
        created_at: from_seconds_to_milliseconds(c.created_at),
        updated_at: from_seconds_to_milliseconds(c.updated_at),
        reviewed_at: from_seconds_to_milliseconds(c.reviewed_at),
        is_on_sale: c.search_is_store_minter,
        size: c.items_count,
        network,
        chain_id,
        first_listed_at: c.first_listed_at.map(from_seconds_to_milliseconds),
    }
}
