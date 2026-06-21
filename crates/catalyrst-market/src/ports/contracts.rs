use catalyrst_server::cache::ResponseCache;
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

use crate::dcl_schemas::{
    ethereum_chain_id, get_db_networks, polygon_chain_id, Contract, Network, NftCategory,
};
use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip};
use crate::marketplace_contracts::get_marketplace_contracts;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Default)]
pub struct ContractFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<ContractSortBy>,
    pub category: Option<NftCategory>,
    pub network: Option<Network>,
}

#[derive(Debug, Clone, Copy)]
pub enum ContractSortBy {
    Name,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct DbCollection {
    pub id: String,
    pub name: String,
    pub chain_id: i32,
    pub network: String,
}

pub struct ContractsComponent {
    pool: PgPool,
    cache: Arc<ResponseCache<(), Vec<Contract>>>,
}

impl ContractsComponent {
    pub fn new(pool: PgPool) -> Self {
        const TTL: Duration = Duration::from_secs(60 * 60);
        Self {
            pool,
            cache: Arc::new(ResponseCache::new("contracts.all_collections", TTL, 1)),
        }
    }

    pub fn get_marketplace_contracts(&self) -> Vec<Contract> {
        get_marketplace_contracts(ethereum_chain_id())
    }

    pub async fn get_collection_contracts(
        &self,
        filters: &ContractFilters,
    ) -> Result<(Vec<Contract>, i64), ApiError> {
        let limit = clamp_first(filters.first, 1000);
        let offset = clamp_skip(filters.skip);

        let networks: Option<Vec<String>> = filters
            .network
            .map(|net| get_db_networks(net).into_iter().map(String::from).collect());

        let where_sql = build_where(networks.is_some());

        let select_sql = format!(
            "SELECT c.id, c.name, c.chain_id::int4 AS chain_id, c.network \
             FROM {schema}.collection c \
             {where_} \
             ORDER BY c.name COLLATE \"C\" ASC \
             LIMIT $1 OFFSET $2",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = where_sql,
        );

        let mut q = sqlx::query_as::<_, DbCollection>(&select_sql)
            .bind(limit)
            .bind(offset);
        if let Some(ref nets) = networks {
            q = q.bind(nets);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let count_sql = format!(
            "SELECT COUNT(c.id) AS count FROM {schema}.collection c {where_}",
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_ = build_where_count(networks.is_some()),
        );
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(ref nets) = networks {
            cq = cq.bind(nets);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let contracts: Vec<Contract> = rows.iter().map(from_db_collection_to_contract).collect();
        Ok((contracts, total))
    }

    pub async fn get_all_collection_contracts(&self) -> Result<Vec<Contract>, ApiError> {
        self.cache
            .get_or_fetch((), || async {
                let count_sql = format!(
                    "SELECT COUNT(c.id) FROM {schema}.collection c WHERE c.is_approved = true",
                    schema = MARKETPLACE_SQUID_SCHEMA
                );
                let total: i64 = sqlx::query_scalar(&count_sql)
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or(0);

                let mut all: Vec<Contract> = Vec::with_capacity(total as usize);
                const PAGE_SIZE: i64 = 500;
                let mut skip: i64 = 0;
                while skip < total {
                    let (page, _) = self
                        .get_collection_contracts(&ContractFilters {
                            first: Some(PAGE_SIZE),
                            skip: Some(skip),
                            ..Default::default()
                        })
                        .await?;
                    all.extend(page);
                    skip += PAGE_SIZE;
                }

                Ok::<_, ApiError>(all)
            })
            .await
    }

    pub async fn get_contracts(
        &self,
        filters: &ContractFilters,
    ) -> Result<(Vec<Contract>, i64), ApiError> {
        let mut marketplace = self.get_marketplace_contracts();
        if let Some(c) = filters.category {
            marketplace.retain(|cn| cn.category == c);
        }
        if let Some(n) = filters.network {
            marketplace.retain(|cn| cn.network == n);
        }

        let should_fetch_all = filters.category.is_none_or(|c| c == NftCategory::Wearable)
            && filters.network.is_none_or(|n| n == Network::Matic);
        let collections = if should_fetch_all {
            self.get_all_collection_contracts().await?
        } else {
            Vec::new()
        };

        let mut all: Vec<Contract> = Vec::with_capacity(marketplace.len() + collections.len());
        all.extend(marketplace);
        all.extend(collections);

        let total = all.len() as i64;
        let first = filters.first.unwrap_or(0);
        let skip = filters.skip.unwrap_or(0).max(0) as usize;
        let sliced: Vec<Contract> = if first <= 0 {
            all.into_iter().skip(skip).collect()
        } else {
            all.into_iter().skip(skip).take(first as usize).collect()
        };

        Ok((sliced, total))
    }
}

fn from_db_collection_to_contract(c: &DbCollection) -> Contract {
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
    Contract {
        name: c.name.clone(),
        address: c.id.clone(),
        category: NftCategory::Wearable,
        network,
        chain_id,
    }
}

fn build_where(has_network_filter: bool) -> String {
    let mut parts = vec!["c.is_approved = true".to_string()];
    if has_network_filter {
        parts.push("c.network = ANY($3::text[])".to_string());
    }
    format!(" WHERE {} ", parts.join(" AND "))
}

fn build_where_count(has_network_filter: bool) -> String {
    let mut parts = vec!["c.is_approved = true".to_string()];
    if has_network_filter {
        parts.push("c.network = ANY($1::text[])".to_string());
    }
    format!(" WHERE {} ", parts.join(" AND "))
}

pub fn parse_filters(
    pairs: &[(String, String)],
) -> Result<ContractFilters, crate::http::errors::InvalidParameterError> {
    use crate::http::params::Params;
    let p = Params::new(pairs);

    let first = p.get_number("first", None).map(|f| f as i64);
    let skip = p.get_number("skip", None).map(|f| f as i64);

    let category = p
        .get_value(
            "category",
            &["parcel", "estate", "wearable", "ens", "emote"],
            None,
        )
        .map(|s| match s.as_str() {
            "parcel" => NftCategory::Parcel,
            "estate" => NftCategory::Estate,
            "wearable" => NftCategory::Wearable,
            "ens" => NftCategory::Ens,
            "emote" => NftCategory::Emote,
            _ => unreachable!(),
        });

    let network = p
        .get_value("network", &["ETHEREUM", "MATIC"], None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            "MATIC" => Network::Matic,
            _ => unreachable!(),
        });

    Ok(ContractFilters {
        first,
        skip,
        sort_by: None,
        category,
        network,
    })
}
