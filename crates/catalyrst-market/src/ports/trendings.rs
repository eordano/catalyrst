//! Direct port of `marketplace-server/src/ports/trendings/{component,queries,types,utils}.ts`.
//!
//! Scope: the upstream component composes the items port to enrich the
//! `(contract_address, item_id)` pairs returned by `getTrendingSalesQuery` into
//! full `Item` records. The items port is owned by a different agent; until it
//! lands here we expose just the raw trending pairs, in the same Item-like
//! envelope (`itemId`, `contractAddress`, `salesCount`) so the API surface
//! stays stable.

use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::MARKETPLACE_SQUID_SCHEMA;

const DEFAULT_SIZE: i64 = 20;

#[derive(Debug, Clone, Default)]
pub struct TrendingFilters {
    pub size: Option<i64>,
    pub picked_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrendingSale {
    #[serde(rename = "itemId")]
    pub item_id: Option<String>,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "salesCount")]
    pub sales_count: i64,
}

pub struct TrendingsComponent {
    pool: PgPool,
}

impl TrendingsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch(&self, filters: &TrendingFilters) -> Result<Vec<TrendingSale>, ApiError> {
        let from = (Utc::now() - Duration::days(1))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();

        let sql = format!(
            r#"
SELECT
  search_item_id,
  search_contract_address,
  COUNT(*) AS sales_count
FROM {schema}.sale
WHERE timestamp > $1
GROUP BY search_item_id, search_contract_address
ORDER BY sales_count DESC
LIMIT $2
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
        );

        let size = filters.size.unwrap_or(DEFAULT_SIZE);
        let rows = sqlx::query_as::<_, (Option<String>, String, i64)>(&sql)
            .bind(from)
            .bind(size)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|(item_id, contract_address, sales_count)| TrendingSale {
                item_id,
                contract_address,
                sales_count,
            })
            .collect())
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<TrendingFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    Ok(TrendingFilters {
        size: p.get_number("size", None).map(|v| v as i64),
        picked_by: p.get_string("pickedBy", None),
    })
}
