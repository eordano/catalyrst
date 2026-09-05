use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct SquidNft {
    pub metadata_id: String,

    pub category: String,

    pub owner_address: String,
    pub search_text: String,
    pub distance_to_plaza: Option<i32>,
    pub adjacent_to_road: Option<bool>,
    pub estate_size: Option<i32>,

    pub created_at: i64,

    pub updated_at: i64,
}

#[derive(Clone)]
pub struct SquidMarketplace {
    pool: PgPool,
    schema: String,
}

impl SquidMarketplace {
    pub fn new(pool: PgPool, schema: String) -> Self {
        Self { pool, schema }
    }

    pub async fn nft_by_contract_token(
        &self,
        contract_address: &str,
        token_id: &str,
    ) -> Result<Option<SquidNft>, sqlx::Error> {
        let sql = format!(
            "SELECT category, owner_address, COALESCE(search_text, '') AS search_text, \
                    search_distance_to_plaza, search_adjacent_to_road, search_estate_size, \
                    COALESCE(created_at, 0)::bigint AS created_at, \
                    COALESCE(updated_at, 0)::bigint AS updated_at \
             FROM {schema}.nft \
             WHERE lower(contract_address) = lower($1) AND token_id = $2::numeric \
             LIMIT 1",
            schema = self.schema
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(contract_address)
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| {
            let owner: Option<String> = r.try_get("owner_address").ok();
            SquidNft {
                metadata_id: format!("{}-{}", contract_address.to_lowercase(), token_id),
                category: r.try_get("category").unwrap_or_default(),
                owner_address: owner.unwrap_or_default().to_lowercase(),
                search_text: r.try_get("search_text").unwrap_or_default(),
                distance_to_plaza: r.try_get("search_distance_to_plaza").ok(),
                adjacent_to_road: r.try_get("search_adjacent_to_road").ok(),
                estate_size: r.try_get("search_estate_size").ok(),
                created_at: r.try_get("created_at").unwrap_or(0),
                updated_at: r.try_get("updated_at").unwrap_or(0),
            }
        }))
    }
}
