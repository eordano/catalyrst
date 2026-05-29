//! Direct port of `marketplace-server/src/ports/stats/{component,queries,types,utils}.ts`.
//!
//! Currently only `category=estate, stat=size` is implemented upstream; we
//! mirror that exact surface. The query rides on top of the nfts port (out of
//! scope for this agent), so the SQL is inlined to pull `size` directly from
//! `estate` rows without going through the full NFT filter pipeline.

use serde::Serialize;
use sqlx::PgPool;
use std::collections::BTreeMap;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsCategory {
    Estate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStats {
    Size,
}

#[derive(Debug, Clone, Default)]
pub struct StatsResourceFilters {
    pub is_on_sale: bool,
    pub adjacent_to_road: bool,
    pub min_distance_to_plaza: Option<f64>,
    pub max_distance_to_plaza: Option<f64>,
    pub max_estate_size: Option<f64>,
    pub min_estate_size: Option<f64>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
}

/// `Record<number, number>` — keyed by size (the int comes back as a JSON
/// string when serialised, matching the upstream).
pub type StatsResponse = BTreeMap<String, i64>;

pub struct StatsComponent {
    pool: PgPool,
}

impl StatsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch(
        &self,
        category: Option<StatsCategory>,
        stat: Option<ResourceStats>,
        filters: &StatsResourceFilters,
    ) -> Result<StatsResponse, ApiError> {
        match (category, stat) {
            (Some(StatsCategory::Estate), Some(ResourceStats::Size)) => {
                self.fetch_estate_sizes(filters).await
            }
            _ => Ok(StatsResponse::new()),
        }
    }

    async fn fetch_estate_sizes(
        &self,
        filters: &StatsResourceFilters,
    ) -> Result<StatsResponse, ApiError> {
        let mut where_parts: Vec<String> = vec!["nft.category = 'estate'".to_string()];
        if filters.is_on_sale {
            where_parts.push("nft.search_order_status = 'open'".to_string());
            where_parts.push("nft.search_order_expires_at > EXTRACT(EPOCH FROM NOW()) * 1000".to_string());
        }
        if filters.adjacent_to_road {
            where_parts.push("estate.adjacent_to_road = true".to_string());
        }
        if let Some(v) = filters.min_distance_to_plaza {
            where_parts.push(format!("estate.distance_to_plaza >= {}", v as i64));
        }
        if let Some(v) = filters.max_distance_to_plaza {
            where_parts.push(format!("estate.distance_to_plaza <= {}", v as i64));
        }
        if let Some(v) = filters.min_estate_size {
            where_parts.push(format!("estate.size >= {}", v as i64));
        }
        if let Some(v) = filters.max_estate_size {
            where_parts.push(format!("estate.size <= {}", v as i64));
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let sql = format!(
            r#"
SELECT estate.size::int4 AS size
FROM {schema}.estate AS estate
JOIN {schema}.nft AS nft ON nft.id = estate.nft_id
{where_sql}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            where_sql = where_sql,
        );

        let sizes: Vec<i32> = sqlx::query_scalar(&sql)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let mut tally: BTreeMap<String, i64> = BTreeMap::new();
        for s in sizes {
            *tally.entry(s.to_string()).or_insert(0) += 1;
        }
        Ok(tally)
    }
}

pub fn parse_category(s: &str) -> Option<StatsCategory> {
    match s {
        "estate" => Some(StatsCategory::Estate),
        _ => None,
    }
}

pub fn parse_stat(s: &str) -> Option<ResourceStats> {
    match s {
        "size" => Some(ResourceStats::Size),
        _ => None,
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<StatsResourceFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    Ok(StatsResourceFilters {
        is_on_sale: p.get_boolean("isOnSale"),
        adjacent_to_road: p.get_boolean("adjacentToRoad"),
        min_distance_to_plaza: p.get_number("minDistanceToPlaza", None),
        max_distance_to_plaza: p.get_number("maxDistanceToPlaza", None),
        max_estate_size: p.get_number("maxEstateSize", None),
        min_estate_size: p.get_number("minEstateSize", None),
        min_price: p.get_string("minPrice", None),
        max_price: p.get_string("maxPrice", None),
    })
}

#[derive(Debug, Serialize)]
pub struct StatsEnvelope {
    pub data: StatsResponse,
}
