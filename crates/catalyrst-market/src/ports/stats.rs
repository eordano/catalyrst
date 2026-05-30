use serde::Serialize;
use sqlx::PgPool;
use std::collections::BTreeMap;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::prices::NumericKey;
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

// NumericKey-keyed so the estate-size histogram serializes in NUMERIC order
// ("2" before "10"). Upstream marketplace-server returns it via a JS object whose
// small-integer keys (<2^32) are iterated in ascending numeric order; a plain
// BTreeMap<String,_> would sort lexicographically ("10" before "2"). See prices.rs.
pub type StatsResponse = BTreeMap<NumericKey, i64>;

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
        let mut inner_where: Vec<String> = vec![
            "nft.search_is_land = TRUE".to_string(),
            "nft.category = 'estate'".to_string(),
        ];
        if let Some(v) = filters.min_estate_size {
            inner_where.push(format!("nft.search_estate_size >= {}", v as i64));
        } else {
            inner_where.push("nft.search_estate_size > 0".to_string());
        }
        if let Some(v) = filters.max_estate_size {
            inner_where.push(format!("nft.search_estate_size <= {}", v as i64));
        }
        if let Some(v) = filters.min_distance_to_plaza {
            inner_where.push(format!("nft.search_distance_to_plaza >= {}", v as i64));
        }
        if let Some(v) = filters.max_distance_to_plaza {
            inner_where.push(format!("nft.search_distance_to_plaza <= {}", v as i64));
        }
        if filters.adjacent_to_road {
            inner_where.push("nft.search_adjacent_to_road = true".to_string());
        }
        if let Some(ref v) = filters.min_price {
            inner_where.push(format!(
                "nft.search_order_price >= {}",
                sanitize_numeric(v)
            ));
        }
        if let Some(ref v) = filters.max_price {
            inner_where.push(format!(
                "nft.search_order_price <= {}",
                sanitize_numeric(v)
            ));
        }

        let limit: i64 = 100;

        let sql = if filters.is_on_sale {
            inner_where.push("nft.active_order_id IS NOT NULL".to_string());
            inner_where.push("o.status = 'open'".to_string());
            inner_where.push("o.expires_at_normalized > NOW()".to_string());

            format!(
                r#"
SELECT nft.search_estate_size AS size
FROM {schema}.nft AS nft
JOIN {schema}."order" AS o ON nft.active_order_id = o.id
WHERE {where_}
ORDER BY nft.created_at DESC
LIMIT {limit}
"#,
                schema = MARKETPLACE_SQUID_SCHEMA,
                where_ = inner_where.join(" AND "),
                limit = limit,
            )
        } else {
            format!(
                r#"
SELECT nft.search_estate_size AS size
FROM {schema}.nft AS nft
WHERE {where_}
ORDER BY nft.created_at DESC
LIMIT {limit}
"#,
                schema = MARKETPLACE_SQUID_SCHEMA,
                where_ = inner_where.join(" AND "),
                limit = limit,
            )
        };

        let sizes: Vec<Option<i32>> = sqlx::query_scalar(&sql)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let mut tally: StatsResponse = BTreeMap::new();
        for s in sizes.into_iter().flatten() {
            *tally.entry(NumericKey(s.to_string())).or_insert(0) += 1;
        }
        Ok(tally)
    }
}

fn sanitize_numeric(s: &str) -> String {
    let trimmed = s.trim();
    let bytes = trimmed.as_bytes();
    let mut start = 0;
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        start = 1;
    }
    let body = &trimmed[start..];
    if !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.') {
        trimmed.to_string()
    } else {
        "0".to_string()
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

pub fn parse_filters(
    pairs: &[(String, String)],
) -> Result<StatsResourceFilters, InvalidParameterError> {
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
