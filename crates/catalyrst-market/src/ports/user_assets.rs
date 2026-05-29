use serde::{Deserialize, Serialize};
use sqlx::types::Json as SqlxJson;
use sqlx::PgPool;

use crate::http::response::ApiError;

pub const FIRST_DEFAULT: i64 = 100;
pub const SKIP_DEFAULT: i64 = 0;

#[derive(Debug, Clone, Default)]
pub struct UserAssetsFilters {
    pub first: i64,
    pub skip: i64,
    pub category: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub order_by: Option<String>,
    pub direction: Option<String>,
    pub item_type: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileWearable {
    pub urn: String,
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub category: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: Option<String>,
    pub name: String,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileEmote {
    pub urn: String,
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub category: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: Option<String>,
    pub name: String,
    pub rarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileName {
    pub name: String,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UrnToken {
    pub urn: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NameOnly {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupedWearable {
    pub urn: String,
    pub amount: i64,
    #[serde(rename = "individualData")]
    pub individual_data: Vec<IndividualData>,
    pub name: String,
    pub rarity: String,
    #[serde(rename = "minTransferredAt")]
    pub min_transferred_at: i64,
    #[serde(rename = "maxTransferredAt")]
    pub max_transferred_at: i64,
    pub category: String,
    #[serde(rename = "itemType")]
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupedEmote {
    pub urn: String,
    pub amount: i64,
    #[serde(rename = "individualData")]
    pub individual_data: Vec<IndividualData>,
    pub name: String,
    pub rarity: String,
    #[serde(rename = "minTransferredAt")]
    pub min_transferred_at: i64,
    #[serde(rename = "maxTransferredAt")]
    pub max_transferred_at: i64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndividualData {
    pub id: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "transferredAt")]
    pub transferred_at: String,
    pub price: String,
}

pub struct UserAssetsComponent {
    pool: PgPool,
}

impl UserAssetsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_wearables_by_owner(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<ProfileWearable>, i64, i64), ApiError> {
        let data_sql = "\
            SELECT \
              nft.id, \
              nft.contract_address, \
              nft.token_id::text AS token_id, \
              nft.network, \
              nft.created_at::int8 AS created_at, \
              nft.updated_at::int8 AS updated_at, \
              nft.urn, \
              owner_address AS owner, \
              nft.image, \
              nft.item_id, \
              wearable.category AS category, \
              wearable.rarity AS rarity, \
              wearable.name AS name, \
              nft.item_type, \
              wearable.description, \
              transferred_at::int8 AS transferred_at, \
              item.price::text AS price \
            FROM squid_marketplace.nft nft \
            LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
            LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
            LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
            WHERE owner_address = $1 \
              AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1') \
            ORDER BY nft.created_at DESC \
            LIMIT $2 OFFSET $3";

        let rows: Vec<ProfileRow> = sqlx::query_as(data_sql)
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let unique_sql = "\
            SELECT COUNT(DISTINCT nft.item_id) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')";
        let total_items: i64 = sqlx::query_scalar(unique_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows.into_iter().map(from_db_row_to_wearable).collect();
        Ok((data, total, total_items))
    }

    pub async fn get_owned_wearables_urn_and_token_id(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<UrnToken>, i64), ApiError> {
        let data_sql = "\
            SELECT nft.urn, nft.token_id::text AS token_id \
            FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1') \
            ORDER BY nft.created_at DESC \
            LIMIT $2 OFFSET $3";
        let rows: Vec<(String, String)> = sqlx::query_as(data_sql)
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|(urn, token_id)| UrnToken {
                urn: fix_urn(&urn),
                token_id,
            })
            .collect();
        Ok((data, total))
    }

    pub async fn get_emotes_by_owner(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<ProfileEmote>, i64, i64), ApiError> {
        let data_sql = "\
            SELECT \
              nft.id, \
              nft.contract_address, \
              nft.token_id::text AS token_id, \
              nft.network, \
              nft.created_at::int8 AS created_at, \
              nft.updated_at::int8 AS updated_at, \
              nft.urn, \
              owner_address AS owner, \
              nft.image, \
              nft.item_id, \
              emote.category AS category, \
              emote.rarity AS rarity, \
              emote.name AS name, \
              nft.item_type, \
              emote.description, \
              transferred_at::int8 AS transferred_at, \
              item.price::text AS price \
            FROM squid_marketplace.nft nft \
            LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
            LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
            WHERE owner_address = $1 \
              AND nft.item_type = 'emote_v1' \
            ORDER BY nft.created_at DESC \
            LIMIT $2 OFFSET $3";

        let rows: Vec<ProfileRow> = sqlx::query_as(data_sql)
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type = 'emote_v1'";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let unique_sql = "\
            SELECT COUNT(DISTINCT nft.item_id) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type = 'emote_v1'";
        let total_items: i64 = sqlx::query_scalar(unique_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows.into_iter().map(from_db_row_to_emote).collect();
        Ok((data, total, total_items))
    }

    pub async fn get_owned_emotes_urn_and_token_id(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<UrnToken>, i64), ApiError> {
        let data_sql = "\
            SELECT nft.urn, nft.token_id::text AS token_id \
            FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type = 'emote_v1' \
            ORDER BY nft.created_at DESC \
            LIMIT $2 OFFSET $3";
        let rows: Vec<(String, String)> = sqlx::query_as(data_sql)
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.item_type = 'emote_v1'";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|(urn, token_id)| UrnToken {
                urn: fix_urn(&urn),
                token_id,
            })
            .collect();
        Ok((data, total))
    }

    pub async fn get_names_by_owner(
        &self,
        owner: &str,
        filters: &UserAssetsFilters,
    ) -> Result<(Vec<ProfileName>, i64), ApiError> {
        let owner_lc = owner.to_lowercase();
        let data_sql = "\
            SELECT \
              nft.id, \
              nft.contract_address, \
              nft.token_id::text AS token_id, \
              nft.network, \
              nft.created_at::int8 AS created_at, \
              nft.updated_at::int8 AS updated_at, \
              nft.urn, \
              owner_address AS owner, \
              nft.image, \
              nft.item_id, \
              nft.category, \
              NULL::text AS rarity, \
              ens.subdomain AS name, \
              nft.item_type, \
              NULL::text AS description, \
              transferred_at::int8 AS transferred_at, \
              orders.price::text AS price \
            FROM squid_marketplace.nft nft \
            LEFT JOIN squid_marketplace.ens ens ON ens.id = nft.ens_id \
            LEFT JOIN squid_marketplace.order orders ON orders.id = nft.active_order_id \
            WHERE owner_address = $1 \
              AND nft.category = 'ens' \
            ORDER BY nft.id ASC \
            LIMIT $2 OFFSET $3";

        let rows: Vec<ProfileRow> = sqlx::query_as(data_sql)
            .bind(&owner_lc)
            .bind(filters.first)
            .bind(filters.skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.category = 'ens'";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(&owner_lc)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows.into_iter().map(from_db_row_to_name).collect();
        Ok((data, total))
    }

    pub async fn get_owned_names_only(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<NameOnly>, i64), ApiError> {
        let owner_lc = owner.to_lowercase();
        let data_sql = "\
            SELECT ens.subdomain AS name \
            FROM squid_marketplace.nft nft \
            LEFT JOIN squid_marketplace.ens ens ON ens.id = nft.ens_id \
            WHERE owner_address = $1 \
              AND nft.category = 'ens' \
            ORDER BY nft.id ASC \
            LIMIT $2 OFFSET $3";
        let rows: Vec<(Option<String>,)> = sqlx::query_as(data_sql)
            .bind(&owner_lc)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = "\
            SELECT COUNT(*) FROM squid_marketplace.nft nft \
            WHERE owner_address = $1 \
              AND nft.category = 'ens'";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(&owner_lc)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let data = rows
            .into_iter()
            .map(|(name,)| NameOnly {
                name: name.unwrap_or_default(),
            })
            .collect();
        Ok((data, total))
    }

    pub async fn get_grouped_wearables_by_owner(
        &self,
        owner: &str,
        filters: &UserAssetsFilters,
    ) -> Result<(Vec<GroupedWearable>, i64), ApiError> {
        let (order_clause, _) = build_order_by_clause(filters);
        let mut bind_idx: usize = 1;
        let mut inner_where = String::new();
        let mut binds: Vec<String> = Vec::new();

        let item_type_in = match &filters.item_type {
            Some(types) if !types.is_empty() => {
                let mut placeholders = Vec::new();
                for t in types {
                    bind_idx += 1;
                    placeholders.push(format!("${}", bind_idx));
                    binds.push(t.clone());
                }
                format!("nft.item_type IN ({})", placeholders.join(", "))
            }
            _ => "nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')".to_string(),
        };
        inner_where.push_str(" AND ");
        inner_where.push_str(&item_type_in);

        if let Some(category) = &filters.category {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND wearable.category = ${}", bind_idx));
            binds.push(category.clone());
        }
        if let Some(rarity) = &filters.rarity {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND wearable.rarity = ${}", bind_idx));
            binds.push(rarity.clone());
        }
        if let Some(name) = &filters.name {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND wearable.name ILIKE ${}", bind_idx));
            binds.push(format!("%{}%", name));
        }

        let outer_where = if let Some(name) = &filters.name {
            bind_idx += 1;
            binds.push(format!("%{}%", name));
            format!(" WHERE name ILIKE ${}", bind_idx)
        } else {
            String::new()
        };

        let limit_idx = bind_idx + 1;
        let offset_idx = bind_idx + 2;

        let data_sql = format!(
            "WITH grouped_wearables AS ( \
                SELECT nft.urn, wearable.category, wearable.rarity, wearable.name, metadata.item_type, \
                    COUNT(*) AS amount, \
                    MIN(nft.transferred_at)::int8 AS min_transferred_at, \
                    MAX(nft.transferred_at)::int8 AS max_transferred_at, \
                    MIN(nft.created_at)::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', nft.urn || ':' || nft.token_id::text, \
                            'tokenId', nft.token_id::text, \
                            'transferredAt', COALESCE(nft.transferred_at, 0)::text, \
                            'price', COALESCE(item.price, 0)::text \
                        ) ORDER BY nft.created_at DESC \
                    ) AS individual_data, \
                    CASE wearable.rarity \
                        WHEN 'unique' THEN 8 WHEN 'mythic' THEN 7 WHEN 'exotic' THEN 6 \
                        WHEN 'legendary' THEN 5 WHEN 'epic' THEN 4 WHEN 'rare' THEN 3 \
                        WHEN 'uncommon' THEN 2 WHEN 'common' THEN 1 ELSE 0 \
                    END AS rarity_order \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
                LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 {inner_where} \
                GROUP BY nft.urn, wearable.category, wearable.rarity, wearable.name, metadata.item_type \
            ) SELECT * FROM grouped_wearables {outer_where} {order} LIMIT ${limit_idx} OFFSET ${offset_idx}",
            inner_where = inner_where,
            outer_where = outer_where,
            order = order_clause,
            limit_idx = limit_idx,
            offset_idx = offset_idx,
        );

        let mut q = sqlx::query_as::<_, GroupedWearableRow>(&data_sql).bind(owner);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(filters.first).bind(filters.skip);
        let rows = q.fetch_all(&self.pool).await?;

        let mut count_bind_idx: usize = 1;
        let mut count_where = String::new();
        let mut count_binds: Vec<String> = Vec::new();

        if let Some(category) = &filters.category {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND wearable.category = ${}", count_bind_idx));
            count_binds.push(category.clone());
        }
        if let Some(rarity) = &filters.rarity {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND wearable.rarity = ${}", count_bind_idx));
            count_binds.push(rarity.clone());
        }
        if let Some(name) = &filters.name {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND wearable.name ILIKE ${}", count_bind_idx));
            count_binds.push(format!("%{}%", name));
        }
        let count_item_type = match &filters.item_type {
            Some(types) if !types.is_empty() => {
                let mut placeholders = Vec::new();
                for t in types {
                    count_bind_idx += 1;
                    placeholders.push(format!("${}", count_bind_idx));
                    count_binds.push(t.clone());
                }
                format!(" AND nft.item_type IN ({})", placeholders.join(", "))
            }
            _ => " AND nft.item_type IN ('wearable_v1', 'wearable_v2', 'smart_wearable_v1')"
                .to_string(),
        };

        let count_sql = format!(
            "SELECT COUNT(DISTINCT nft.urn) FROM squid_marketplace.nft nft \
             LEFT JOIN squid_marketplace.metadata metadata ON nft.metadata_id = metadata.id \
             LEFT JOIN squid_marketplace.wearable wearable ON metadata.wearable_id = wearable.id \
             WHERE owner_address = $1 {} {}",
            count_where, count_item_type,
        );
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql).bind(owner);
        for b in &count_binds {
            cq = cq.bind(b);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let data = rows.into_iter().map(from_grouped_row_to_wearable).collect();
        Ok((data, total))
    }

    pub async fn get_grouped_emotes_by_owner(
        &self,
        owner: &str,
        filters: &UserAssetsFilters,
    ) -> Result<(Vec<GroupedEmote>, i64), ApiError> {
        let (order_clause, _) = build_order_by_clause(filters);
        let mut bind_idx: usize = 1;
        let mut inner_where = String::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(category) = &filters.category {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND emote.category = ${}", bind_idx));
            binds.push(category.clone());
        }
        if let Some(rarity) = &filters.rarity {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND emote.rarity = ${}", bind_idx));
            binds.push(rarity.clone());
        }
        if let Some(name) = &filters.name {
            bind_idx += 1;
            inner_where.push_str(&format!(" AND emote.name ILIKE ${}", bind_idx));
            binds.push(format!("%{}%", name));
        }

        let outer_where = if let Some(name) = &filters.name {
            bind_idx += 1;
            binds.push(format!("%{}%", name));
            format!(" WHERE name ILIKE ${}", bind_idx)
        } else {
            String::new()
        };

        let limit_idx = bind_idx + 1;
        let offset_idx = bind_idx + 2;

        let data_sql = format!(
            "WITH grouped_emotes AS ( \
                SELECT nft.urn, emote.category, emote.rarity, emote.name, \
                    COUNT(*) AS amount, \
                    MIN(nft.transferred_at)::int8 AS min_transferred_at, \
                    MAX(nft.transferred_at)::int8 AS max_transferred_at, \
                    MIN(nft.created_at)::int8 AS min_created_at, \
                    JSON_AGG( \
                        JSON_BUILD_OBJECT( \
                            'id', nft.urn || ':' || nft.token_id::text, \
                            'tokenId', nft.token_id::text, \
                            'transferredAt', COALESCE(nft.transferred_at, 0)::text, \
                            'price', COALESCE(item.price, 0)::text \
                        ) ORDER BY nft.created_at DESC \
                    ) AS individual_data, \
                    CASE emote.rarity \
                        WHEN 'unique' THEN 8 WHEN 'mythic' THEN 7 WHEN 'exotic' THEN 6 \
                        WHEN 'legendary' THEN 5 WHEN 'epic' THEN 4 WHEN 'rare' THEN 3 \
                        WHEN 'uncommon' THEN 2 WHEN 'common' THEN 1 ELSE 0 \
                    END AS rarity_order \
                FROM squid_marketplace.nft nft \
                LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
                LEFT JOIN squid_marketplace.item item ON nft.item_id = item.id \
                WHERE owner_address = $1 \
                  AND nft.item_type = 'emote_v1' {inner_where} \
                GROUP BY nft.urn, emote.category, emote.rarity, emote.name \
            ) SELECT * FROM grouped_emotes {outer_where} {order} LIMIT ${limit_idx} OFFSET ${offset_idx}",
            inner_where = inner_where,
            outer_where = outer_where,
            order = order_clause,
            limit_idx = limit_idx,
            offset_idx = offset_idx,
        );

        let mut q = sqlx::query_as::<_, GroupedEmoteRow>(&data_sql).bind(owner);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(filters.first).bind(filters.skip);
        let rows = q.fetch_all(&self.pool).await?;

        let mut count_bind_idx: usize = 1;
        let mut count_where = String::new();
        let mut count_binds: Vec<String> = Vec::new();
        if let Some(category) = &filters.category {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND emote.category = ${}", count_bind_idx));
            count_binds.push(category.clone());
        }
        if let Some(rarity) = &filters.rarity {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND emote.rarity = ${}", count_bind_idx));
            count_binds.push(rarity.clone());
        }
        if let Some(name) = &filters.name {
            count_bind_idx += 1;
            count_where.push_str(&format!(" AND emote.name ILIKE ${}", count_bind_idx));
            count_binds.push(format!("%{}%", name));
        }
        let _ = count_bind_idx;

        let count_sql = format!(
            "SELECT COUNT(DISTINCT nft.urn) FROM squid_marketplace.nft nft \
             LEFT JOIN squid_marketplace.emote emote ON nft.item_id = emote.id \
             WHERE owner_address = $1 AND nft.item_type = 'emote_v1' {}",
            count_where,
        );
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql).bind(owner);
        for b in &count_binds {
            cq = cq.bind(b);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let data = rows.into_iter().map(from_grouped_row_to_emote).collect();
        Ok((data, total))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ProfileRow {
    id: String,
    contract_address: Option<String>,
    token_id: String,
    #[allow(dead_code)]
    network: Option<String>,
    #[allow(dead_code)]
    created_at: Option<i64>,
    #[allow(dead_code)]
    updated_at: Option<i64>,
    urn: Option<String>,
    #[allow(dead_code)]
    owner: Option<String>,
    #[allow(dead_code)]
    image: Option<String>,
    #[allow(dead_code)]
    item_id: Option<String>,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    #[allow(dead_code)]
    item_type: Option<String>,
    #[allow(dead_code)]
    description: Option<String>,
    transferred_at: Option<i64>,
    price: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct GroupedWearableRow {
    urn: String,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    item_type: Option<String>,
    amount: i64,
    min_transferred_at: Option<i64>,
    max_transferred_at: Option<i64>,
    #[allow(dead_code)]
    min_created_at: Option<i64>,
    individual_data: SqlxJson<Vec<IndividualData>>,
    #[allow(dead_code)]
    rarity_order: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct GroupedEmoteRow {
    urn: String,
    category: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    amount: i64,
    min_transferred_at: Option<i64>,
    max_transferred_at: Option<i64>,
    #[allow(dead_code)]
    min_created_at: Option<i64>,
    individual_data: SqlxJson<Vec<IndividualData>>,
    #[allow(dead_code)]
    rarity_order: i32,
}

pub fn fix_urn(urn: &str) -> String {
    urn.replace("mainnet", "ethereum")
}

fn fix_individual_data(individual_data: Vec<IndividualData>) -> Vec<IndividualData> {
    individual_data
        .into_iter()
        .map(|mut row| {
            if row.id.starts_with("urn:decentraland:") {
                row.id = fix_urn(&row.id);
            }
            row
        })
        .collect()
}

fn from_db_row_to_wearable(row: ProfileRow) -> ProfileWearable {
    let transferred_at = Some(match row.transferred_at {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    });
    ProfileWearable {
        urn: fix_urn(&row.urn.clone().unwrap_or_default()),
        id: row.id,
        token_id: row.token_id,

        category: row.category.unwrap_or_else(|| "eyewear".to_string()),
        transferred_at,
        name: row.name.unwrap_or_default(),
        rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
        price: row.price,
    }
}

fn from_db_row_to_emote(row: ProfileRow) -> ProfileEmote {
    let transferred_at = Some(match row.transferred_at {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    });
    ProfileEmote {
        urn: fix_urn(&row.urn.clone().unwrap_or_default()),
        id: row.id,
        token_id: row.token_id,

        category: row.category.unwrap_or_else(|| "dance".to_string()),
        transferred_at,
        name: row.name.unwrap_or_default(),
        rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
        price: row.price,
    }
}

fn from_db_row_to_name(row: ProfileRow) -> ProfileName {
    ProfileName {
        name: row.name.unwrap_or_default(),
        contract_address: row.contract_address.unwrap_or_default(),
        token_id: row.token_id,
        price: row.price,
    }
}

fn from_grouped_row_to_wearable(row: GroupedWearableRow) -> GroupedWearable {
    GroupedWearable {
        urn: fix_urn(&row.urn),
        amount: row.amount,
        individual_data: fix_individual_data(row.individual_data.0),
        name: row.name.unwrap_or_default(),
        rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
        min_transferred_at: row.min_transferred_at.unwrap_or(0),
        max_transferred_at: row.max_transferred_at.unwrap_or(0),
        category: row.category.unwrap_or_else(|| "eyewear".to_string()),
        item_type: row.item_type.unwrap_or_default(),
    }
}

fn from_grouped_row_to_emote(row: GroupedEmoteRow) -> GroupedEmote {
    GroupedEmote {
        urn: fix_urn(&row.urn),
        amount: row.amount,
        individual_data: fix_individual_data(row.individual_data.0),
        name: row.name.unwrap_or_default(),
        rarity: row.rarity.unwrap_or_else(|| "common".to_string()),
        min_transferred_at: row.min_transferred_at.unwrap_or(0),
        max_transferred_at: row.max_transferred_at.unwrap_or(0),
        category: row.category.unwrap_or_else(|| "dance".to_string()),
    }
}

fn build_order_by_clause(filters: &UserAssetsFilters) -> (&'static str, ()) {
    let sort_field = filters
        .order_by
        .as_deref()
        .unwrap_or("rarity")
        .to_lowercase();
    let default_dir = if sort_field == "name" { "ASC" } else { "DESC" };
    let dir = filters
        .direction
        .as_deref()
        .unwrap_or(default_dir)
        .to_uppercase();

    let clause = match sort_field.as_str() {
        "rarity" => {
            if dir == "ASC" {
                " ORDER BY rarity_order ASC, urn ASC"
            } else {
                " ORDER BY rarity_order DESC, urn ASC"
            }
        }
        "name" => {
            if dir == "ASC" {
                " ORDER BY name ASC, urn ASC"
            } else {
                " ORDER BY name DESC, urn ASC"
            }
        }
        "date" => {
            if dir == "ASC" {
                " ORDER BY min_transferred_at ASC, urn DESC"
            } else {
                " ORDER BY max_transferred_at DESC, urn ASC"
            }
        }
        _ => " ORDER BY rarity_order DESC, urn ASC",
    };
    (clause, ())
}

pub fn parse_user_assets_params(pairs: &[(String, String)]) -> UserAssetsFilters {
    use crate::http::params::Params;
    const MAX_LIMIT: i64 = 1000;
    const DEFAULT_LIMIT: i64 = 100;

    let p = Params::new(pairs);

    let limit = p.get_number("limit", None).map(|n| n as i64);
    let offset = p.get_number("offset", None).map(|n| n as i64);
    let first = p.get_number("first", None).map(|n| n as i64);
    let skip = p.get_number("skip", None).map(|n| n as i64);

    let requested_limit = limit.or(first).unwrap_or(DEFAULT_LIMIT);
    let requested_skip = offset.or(skip).unwrap_or(0);
    let capped_limit = requested_limit.min(MAX_LIMIT);

    let item_type_list = p.get_list("itemType", &[]);
    let item_type = if item_type_list.is_empty() {
        None
    } else {
        Some(item_type_list)
    };

    UserAssetsFilters {
        first: capped_limit,
        skip: requested_skip,
        category: p.get_string("category", None),
        rarity: p.get_string("rarity", None),
        name: p.get_string("name", None),
        order_by: p.get_string("orderBy", None),
        direction: p.get_string("direction", None),
        item_type,
    }
}
