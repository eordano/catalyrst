use serde::Serialize;
use sqlx::PgPool;

use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip};
use crate::MARKETPLACE_SQUID_SCHEMA;

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
            "SELECT nft.issued_id::text AS issued_id, account.address AS owner, nft.token_id::text AS token_id, \
                    COUNT(*) OVER() AS total \
             FROM {schema}.nft AS nft \
             LEFT JOIN {schema}.account AS account ON nft.owner_id = account.id \
             WHERE nft.contract_address = $1 AND nft.item_blockchain_id = $2::numeric \
             {order_clause} \
             OFFSET $3 LIMIT $4",
            schema = MARKETPLACE_SQUID_SCHEMA,
            order_clause = order_clause,
        );

        let rows: Vec<(String, String, String, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(select_sql))
                .bind(&filters.contract_address)
                .bind(&filters.item_id)
                .bind(skip)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;

        let total: i64 = match rows.first() {
            Some((_, _, _, total)) => *total,
            None if skip > 0 => {
                let count_sql = format!(
                    "SELECT COUNT(*) FROM {schema}.nft AS nft \
                     LEFT JOIN {schema}.account AS account ON nft.owner_id = account.id \
                     WHERE nft.contract_address = $1 AND nft.item_blockchain_id = $2::numeric",
                    schema = MARKETPLACE_SQUID_SCHEMA,
                );
                sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
                    .bind(&filters.contract_address)
                    .bind(&filters.item_id)
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or(0)
            }
            None => 0,
        };

        let owners = rows
            .into_iter()
            .map(|(issued_id, owner_id, token_id, _)| Owner {
                issued_id,
                owner_id,
                token_id,
            })
            .collect();

        Ok((owners, total))
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use catalyrst_contract_gate::pg::ScratchDb;

    #[tokio::test]
    async fn page_total_rides_the_page_query() {
        let Some(scratch) = ScratchDb::builder("CATALYRST_MARKET_TEST_PG", "owners")
            .schemas(["squid_marketplace"])
            .build()
            .await
        else {
            return;
        };
        scratch
            .apply_sql(
                "CREATE TABLE squid_marketplace.nft (contract_address text, \
                    item_blockchain_id numeric, issued_id numeric, owner_id text, token_id numeric);
                 CREATE TABLE squid_marketplace.account (id text, address text);
                 INSERT INTO squid_marketplace.account VALUES ('acc-1', '0xa'), ('acc-2', '0xb');
                 INSERT INTO squid_marketplace.nft VALUES
                    ('0xc', 7, 1, 'acc-1', 101), ('0xc', 7, 2, 'acc-2', 102),
                    ('0xc', 7, 3, 'acc-1', 103), ('0xc', 8, 1, 'acc-1', 104);",
            )
            .await;
        let c = OwnersComponent::new(scratch.pool.clone());
        let page = |first, skip| OwnersFilters {
            contract_address: "0xc".to_string(),
            item_id: "7".to_string(),
            first: Some(first),
            skip: Some(skip),
            sort_by: Some(OwnersSortBy::IssuedId),
            order_direction: None,
        };

        let (rows, total) = c.fetch_and_count(&page(2, 0)).await.unwrap();
        assert_eq!((rows.len(), total), (2, 3));
        assert_eq!(rows[0].issued_id, "3", "issued_id DESC");
        assert_eq!(rows[0].owner_id, "0xa");
        let (rows, total) = c.fetch_and_count(&page(2, 10)).await.unwrap();
        assert_eq!(
            (rows.len(), total),
            (0, 3),
            "past the end: standalone count"
        );
        let mut none = page(2, 0);
        none.item_id = "9".to_string();
        let (rows, total) = c.fetch_and_count(&none).await.unwrap();
        assert_eq!((rows.len(), total), (0, 0));
        scratch.drop().await;
    }
}
