use serde::Serialize;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip};
use crate::MARKETPLACE_SQUID_SCHEMA;

pub const OWNERS_QUERY_DEFAULT_OFFSET: i64 = 0;
pub const OWNERS_QUERY_DEFAULT_LIMIT: i64 = 20;

#[derive(Debug, Clone, Copy)]
pub enum OwnersSortBy {
    IssuedId,
}

#[derive(Debug, Clone, Default)]
pub struct OwnersFilters {
    pub contract_address: String,
    pub item_id: String,
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<OwnersSortBy>,
    pub order_direction: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "market/", rename_all = "camelCase")
)]
pub struct Owner {
    #[serde(rename = "issuedId")]
    pub issued_id: String,
    #[serde(rename = "ownerId")]
    pub owner_id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

pub struct OwnersComponent {
    pool: PgPool,
}

impl OwnersComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch_and_count(
        &self,
        filters: &OwnersFilters,
    ) -> Result<(Vec<Owner>, i64), ApiError> {
        let order_clause = match filters.sort_by {
            Some(OwnersSortBy::IssuedId) => {
                let dir = filters.order_direction.as_deref().unwrap_or("desc");
                if dir == "asc" {
                    " ORDER BY nft.issued_id ASC"
                } else {
                    " ORDER BY nft.issued_id DESC"
                }
            }
            None => "",
        };

        let skip = clamp_skip(filters.skip);
        let limit = clamp_first(filters.first, OWNERS_QUERY_DEFAULT_LIMIT);

        let select_sql = format!(
            "SELECT nft.issued_id::text AS issued_id, account.address AS owner, nft.token_id::text AS token_id \
             FROM {schema}.nft AS nft \
             LEFT JOIN {schema}.account AS account ON nft.owner_id = account.id \
             WHERE nft.contract_address = $1 AND nft.item_blockchain_id = $2::numeric \
             {order_clause} \
             OFFSET $3 LIMIT $4",
            schema = MARKETPLACE_SQUID_SCHEMA,
            order_clause = order_clause,
        );

        let rows: Vec<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(select_sql))
            .bind(&filters.contract_address)
            .bind(&filters.item_id)
            .bind(skip)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = format!(
            "SELECT COUNT(*) FROM {schema}.nft AS nft \
             LEFT JOIN {schema}.account AS account ON nft.owner_id = account.id \
             WHERE nft.contract_address = $1 AND nft.item_blockchain_id = $2::numeric",
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
            .bind(&filters.contract_address)
            .bind(&filters.item_id)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let owners = rows
            .into_iter()
            .map(|(issued_id, owner_id, token_id)| Owner {
                issued_id,
                owner_id,
                token_id,
            })
            .collect();

        Ok((owners, total))
    }
}
