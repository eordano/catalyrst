use sqlx::PgPool;

use crate::http::response::ApiError;

use super::rows::{
    build_order_by_clause, fix_urn, from_db_row_to_emote, from_db_row_to_name,
    from_db_row_to_wearable, from_grouped_row_to_emote, from_grouped_row_to_wearable,
    GroupedEmoteRow, GroupedWearableRow, ProfileRow,
};
use super::sql::{
    emotes_count_sql, emotes_data_sql, emotes_unique_sql, emotes_urn_token_data_sql,
    grouped_emotes_count_sql, grouped_emotes_data_sql, grouped_wearables_count_sql,
    grouped_wearables_data_sql, wearables_count_sql, wearables_data_sql, wearables_unique_sql,
    wearables_urn_token_data_sql,
};
use super::types::{
    GroupedEmote, GroupedWearable, NameOnly, ProfileEmote, ProfileName, ProfileWearable, UrnToken,
    UserAssetsFilters,
};

pub async fn usage_grants_present(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT to_regclass('marketplace.usage_grants') IS NOT NULL \
         AND has_table_privilege(current_user, 'marketplace.usage_grants', 'SELECT')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub struct UserAssetsComponent {
    pool: PgPool,
    grants_present: bool,
}

impl UserAssetsComponent {
    pub fn new(pool: PgPool, grants_present: bool) -> Self {
        Self {
            pool,
            grants_present,
        }
    }

    pub async fn get_wearables_by_owner(
        &self,
        owner: &str,
        first: i64,
        skip: i64,
    ) -> Result<(Vec<(ProfileWearable, bool)>, i64, i64), ApiError> {
        let data_sql = wearables_data_sql(self.grants_present);
        let rows: Vec<ProfileRow> = sqlx::query_as(sqlx::AssertSqlSafe(data_sql))
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = wearables_count_sql(self.grants_present);
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let unique_sql = wearables_unique_sql(self.grants_present);
        let total_items: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(unique_sql))
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
        let data_sql = wearables_urn_token_data_sql(self.grants_present);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(data_sql))
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = wearables_count_sql(self.grants_present);
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
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
    ) -> Result<(Vec<(ProfileEmote, bool)>, i64, i64), ApiError> {
        let data_sql = emotes_data_sql(self.grants_present);
        let rows: Vec<ProfileRow> = sqlx::query_as(sqlx::AssertSqlSafe(data_sql))
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = emotes_count_sql(self.grants_present);
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
            .bind(owner)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        let unique_sql = emotes_unique_sql(self.grants_present);
        let total_items: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(unique_sql))
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
        let data_sql = emotes_urn_token_data_sql(self.grants_present);
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(data_sql))
            .bind(owner)
            .bind(first)
            .bind(skip)
            .fetch_all(&self.pool)
            .await?;

        let count_sql = emotes_count_sql(self.grants_present);
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
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
    ) -> Result<(Vec<(GroupedWearable, bool)>, i64), ApiError> {
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

        let data_sql = grouped_wearables_data_sql(
            self.grants_present,
            &inner_where,
            &outer_where,
            order_clause,
            limit_idx,
            offset_idx,
        );

        let mut q =
            sqlx::query_as::<_, GroupedWearableRow>(sqlx::AssertSqlSafe(data_sql)).bind(owner);
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

        let count_sql =
            grouped_wearables_count_sql(self.grants_present, &count_where, &count_item_type);
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql)).bind(owner);
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
    ) -> Result<(Vec<(GroupedEmote, bool)>, i64), ApiError> {
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

        let data_sql = grouped_emotes_data_sql(
            self.grants_present,
            &inner_where,
            &outer_where,
            order_clause,
            limit_idx,
            offset_idx,
        );

        let mut q = sqlx::query_as::<_, GroupedEmoteRow>(sqlx::AssertSqlSafe(data_sql)).bind(owner);
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

        let count_sql = grouped_emotes_count_sql(self.grants_present, &count_where);
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql)).bind(owner);
        for b in &count_binds {
            cq = cq.bind(b);
        }
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let data = rows.into_iter().map(from_grouped_row_to_emote).collect();
        Ok((data, total))
    }
}
