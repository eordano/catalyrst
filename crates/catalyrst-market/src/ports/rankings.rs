use serde::Serialize;
use sqlx::PgPool;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::numeric::bn_cmp;
use crate::logic::rankings::{
    get_unique_collectors_from_collectors_day_data, get_unique_creators_from_creators_day_data,
    get_unique_items_from_items_day_data, CollectorRank, CollectorsDayDataFragment, CreatorRank,
    CreatorsDayDataFragment, ItemRank, ItemsDayDataFragment,
};
use crate::MARKETPLACE_SQUID_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankingEntity {
    Wearables,
    Emotes,
    Creators,
    Collectors,
}

impl RankingEntity {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "wearables" => RankingEntity::Wearables,
            "emotes" => RankingEntity::Emotes,
            "creators" => RankingEntity::Creators,
            "collectors" => RankingEntity::Collectors,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankingsSortBy {
    MostVolume,
    MostSales,
}

#[derive(Debug, Clone, Default)]
pub struct RankingsFilters {
    pub from: i64,
    pub first: Option<i64>,
    pub rarity: Option<String>,
    pub category: Option<String>,
    pub sort_by: Option<RankingsSortBy>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RankingResponse {
    Items(Vec<ItemRank>),
    Creators(Vec<CreatorRank>),
    Collectors(Vec<CollectorRank>),
}

pub struct RankingsComponent {
    pool: PgPool,
}

impl RankingsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn fetch(
        &self,
        entity: RankingEntity,
        f: &RankingsFilters,
    ) -> Result<RankingResponse, ApiError> {
        let is_all_time = f.from == 0;
        match entity {
            RankingEntity::Wearables | RankingEntity::Emotes => {
                let fragments = self.fetch_items_sales(entity, f).await?;
                let mut ranks = get_unique_items_from_items_day_data(fragments, f.from);
                if !is_all_time {
                    sort_items(&mut ranks, f.sort_by);
                }
                if let Some(first) = f.first {
                    ranks.truncate(first as usize);
                }
                Ok(RankingResponse::Items(ranks))
            }
            RankingEntity::Creators => {
                let fragments = self.fetch_creators(f).await?;
                let mut ranks = get_unique_creators_from_creators_day_data(fragments);
                if !is_all_time {
                    sort_creators(&mut ranks, f.sort_by);
                }
                if let Some(first) = f.first {
                    ranks.truncate(first as usize);
                }
                Ok(RankingResponse::Creators(ranks))
            }
            RankingEntity::Collectors => {
                let fragments = self.fetch_collectors(f).await?;
                let mut ranks = get_unique_collectors_from_collectors_day_data(fragments);
                if !is_all_time {
                    sort_collectors(&mut ranks, f.sort_by);
                }
                if let Some(first) = f.first {
                    ranks.truncate(first as usize);
                }
                Ok(RankingResponse::Collectors(ranks))
            }
        }
    }

    async fn fetch_items_sales(
        &self,
        entity: RankingEntity,
        f: &RankingsFilters,
    ) -> Result<Vec<ItemsDayDataFragment>, ApiError> {
        let mut where_parts: Vec<String> = Vec::new();
        match entity {
            RankingEntity::Wearables => where_parts.push("search_category = 'wearable'".into()),
            RankingEntity::Emotes => where_parts.push("search_category = 'emote'".into()),
            _ => {}
        }
        let mut binds: Vec<String> = Vec::new();
        let mut bind_i64: Vec<i64> = Vec::new();
        let mut kinds: Vec<char> = Vec::new();
        let mut idx = 0;
        let mut next = || {
            idx += 1;
            format!("${}", idx)
        };
        let needs_item_join = f.rarity.is_some();
        if let Some(ref r) = f.rarity {
            where_parts.push(format!("item.rarity = {}", next()));
            binds.push(r.clone());
            kinds.push('s');
        }
        if f.from != 0 {
            where_parts.push(format!("timestamp >= {}", next()));
            bind_i64.push(f.from / 1000);
            kinds.push('i');
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };
        // NOTE: `volume` is cast to text in the SELECT list, so referring to
        // the alias in ORDER BY would sort lexicographically — that's the bug
        // the prior implementation hit (highest first wins because "9..." > "8...").
        // Use the raw numeric expression here so the sort is numeric.
        // Tie-break by sales (DESC) and item id (ASC) for stability against
        // upstream, mirroring the "ORDER BY volume DESC NULLS LAST, sales DESC, id ASC"
        // contract called out in the parity report.
        let order_by = match f.sort_by {
            Some(RankingsSortBy::MostSales) => {
                "COUNT(*) DESC, COALESCE(SUM(sale.price::numeric), 0) DESC NULLS LAST, nft.item_id ASC"
            }
            _ => {
                "COALESCE(SUM(sale.price::numeric), 0) DESC NULLS LAST, COUNT(*) DESC, nft.item_id ASC"
            }
        };
        let join_item = if needs_item_join {
            format!(
                "LEFT JOIN {schema}.item item ON sale.search_contract_address = item.collection_id AND sale.search_item_id::text = item.blockchain_id::text AND sale.type = 'mint'",
                schema = MARKETPLACE_SQUID_SCHEMA,
            )
        } else {
            String::new()
        };
        let limit_clause = if let Some(first) = f.first {
            format!(" LIMIT {}", first)
        } else {
            String::new()
        };
        let sql = format!(
            r#"
SELECT
  nft.item_id                AS id,
  COUNT(*)::int8             AS sales,
  COALESCE(SUM(sale.price::numeric), 0)::text AS volume
FROM {schema}.sale
{join_item}
LEFT JOIN {schema}.nft ON nft.id = sale.nft_id
{where_sql}
GROUP BY nft.item_id
ORDER BY {order_by}
{limit_clause}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            join_item = join_item,
            where_sql = where_sql,
            order_by = order_by,
            limit_clause = limit_clause,
        );
        let mut q = sqlx::query_as::<_, (Option<String>, i64, String)>(&sql);
        let mut s_iter = binds.iter();
        let mut i_iter = bind_i64.iter();
        for k in &kinds {
            if *k == 's' {
                q = q.bind(s_iter.next().cloned().unwrap_or_default());
            } else {
                q = q.bind(*i_iter.next().unwrap_or(&0));
            }
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, sales, volume)| {
                id.map(|id| ItemsDayDataFragment { id, sales, volume })
            })
            .collect())
    }

    async fn fetch_creators(
        &self,
        f: &RankingsFilters,
    ) -> Result<Vec<CreatorsDayDataFragment>, ApiError> {
        let table = if f.from == 0 {
            "accounts"
        } else {
            "accounts_day_data"
        };
        let order_by = match f.sort_by {
            Some(RankingsSortBy::MostSales) if f.from == 0 => "primary_sales",
            Some(RankingsSortBy::MostSales) => "sales",
            Some(RankingsSortBy::MostVolume) if f.from == 0 => "primary_sales_earned",
            _ => "earned",
        };
        let date_filter = if f.from != 0 {
            format!("AND date >= {}", f.from / 1000)
        } else {
            String::new()
        };
        let coll_filter = if f.from == 0 {
            "AND collections > 0"
        } else {
            ""
        };
        let limit_clause = if let Some(first) = f.first {
            format!("LIMIT {}", first)
        } else {
            String::new()
        };
        let sql = format!(
            r#"
SELECT
  id,
  sales::int8 AS sales,
  earned::text AS earned,
  unique_collections_sales::int8 AS unique_collections_sales,
  unique_collectors_total::int8 AS unique_collectors_total
FROM {schema}.{table}
WHERE sales > 0 {coll_filter} {date_filter}
ORDER BY {order_by} DESC
{limit_clause}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            table = table,
            coll_filter = coll_filter,
            date_filter = date_filter,
            order_by = order_by,
            limit_clause = limit_clause,
        );
        let rows = sqlx::query_as::<_, (String, i64, String, i64, i64)>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(id, sales, earned, ucs, uct)| CreatorsDayDataFragment {
                id,
                sales,
                earned,
                unique_collections_sales: ucs,
                unique_collectors_total: uct,
            })
            .collect())
    }

    async fn fetch_collectors(
        &self,
        f: &RankingsFilters,
    ) -> Result<Vec<CollectorsDayDataFragment>, ApiError> {
        let table = if f.from == 0 {
            "accounts"
        } else {
            "accounts_day_data"
        };
        let order_by = match f.sort_by {
            Some(RankingsSortBy::MostSales) => "purchases",
            _ => "spent",
        };
        let date_filter = if f.from != 0 {
            format!("AND date >= {}", f.from / 1000)
        } else {
            String::new()
        };
        let limit_clause = if let Some(first) = f.first {
            format!("LIMIT {}", first)
        } else {
            String::new()
        };
        let sql = format!(
            r#"
SELECT
  id,
  purchases::int8 AS purchases,
  spent::text AS spent,
  unique_and_mythic_items::int8 AS unique_and_mythic_items,
  creators_supported_total::int8 AS creators_supported_total
FROM {schema}.{table}
WHERE purchases > 0 {date_filter}
ORDER BY {order_by} DESC
{limit_clause}
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
            table = table,
            date_filter = date_filter,
            order_by = order_by,
            limit_clause = limit_clause,
        );
        let rows = sqlx::query_as::<_, (String, i64, String, i64, i64)>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, purchases, spent, uami, cst)| CollectorsDayDataFragment {
                    id,
                    purchases,
                    spent,
                    unique_and_mythic_items: uami,
                    creators_supported_total: cst,
                },
            )
            .collect())
    }
}

fn sort_items(ranks: &mut Vec<ItemRank>, sort_by: Option<RankingsSortBy>) {
    match sort_by {
        Some(RankingsSortBy::MostSales) => ranks.sort_by(|a, b| b.sales.cmp(&a.sales)),
        _ => ranks.sort_by(|a, b| bn_cmp(&b.volume, &a.volume)),
    }
}

fn sort_creators(ranks: &mut Vec<CreatorRank>, sort_by: Option<RankingsSortBy>) {
    match sort_by {
        Some(RankingsSortBy::MostSales) => ranks.sort_by(|a, b| b.sales.cmp(&a.sales)),
        _ => ranks.sort_by(|a, b| bn_cmp(&b.earned, &a.earned)),
    }
}

fn sort_collectors(ranks: &mut Vec<CollectorRank>, sort_by: Option<RankingsSortBy>) {
    match sort_by {
        Some(RankingsSortBy::MostSales) => ranks.sort_by(|a, b| b.purchases.cmp(&a.purchases)),
        _ => ranks.sort_by(|a, b| bn_cmp(&b.spent, &a.spent)),
    }
}

pub fn parse_filters(
    pairs: &[(String, String)],
    from: i64,
) -> Result<RankingsFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    let sort_by = p
        .get_value("sortBy", &["most_volume", "most_sales"], None)
        .map(|s| match s.as_str() {
            "most_sales" => RankingsSortBy::MostSales,
            _ => RankingsSortBy::MostVolume,
        });
    Ok(RankingsFilters {
        from,
        first: p.get_number("first", None).map(|v| v as i64),
        rarity: p.get_string("rarity", None),
        category: p.get_string("category", None),
        sort_by,
    })
}
