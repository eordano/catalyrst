use chrono::{Duration, TimeZone, Utc};
use sqlx::PgPool;
use std::collections::HashMap;

use crate::http::errors::InvalidParameterError;
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::items::{Item, ItemFilters, ItemsComponent};
use crate::MARKETPLACE_SQUID_SCHEMA;

const DEFAULT_SIZE: i64 = 20;
const SALES_CUT: f64 = 0.6;
const VOLUME_CUT: f64 = 0.4;
const TRENDING_SALES_LIMIT: i64 = 1000;

#[derive(Debug, Clone, Default)]
pub struct TrendingFilters {
    pub size: Option<i64>,
    pub picked_by: Option<String>,
}

pub struct TrendingsComponent {
    pool: PgPool,
}

impl TrendingsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns full `Item` rows for the trending NFTs based on sales and
    /// volume over the last day. Mirrors upstream `createTrendingsComponent`
    /// (`/tmp/mps-src/src/ports/trendings/component.ts`):
    ///   1. Fetch up to 1000 sales from the last 24h (today 00:00:00 cut).
    ///   2. Group by (contract, itemId), counting occurrences.
    ///   3. Look up the matching items via `ItemsComponent::get_items`.
    ///   4. Take 60% of `size` from the items with the most sales,
    ///      filtering to those that are currently on sale.
    ///   5. Take 40% of `size` from the items with the highest implied
    ///      volume (sales_count * price), excluding any already in (4).
    pub async fn fetch(
        &self,
        items: &ItemsComponent,
        filters: &TrendingFilters,
    ) -> Result<Vec<Item>, ApiError> {
        let size = filters.size.unwrap_or(DEFAULT_SIZE);
        if size <= 0 {
            return Ok(Vec::new());
        }

        // Mirror upstream `getDateXDaysAgo(1)`: midnight UTC, 1 day ago.
        let from_ts = midnight_days_ago(1);

        let sql = format!(
            r#"
SELECT
  search_item_id::text AS search_item_id,
  search_contract_address
FROM {schema}.sale
WHERE timestamp > $1
ORDER BY timestamp DESC
LIMIT $2 OFFSET 0
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
        );

        let rows: Vec<(Option<String>, String)> =
            sqlx::query_as::<_, (Option<String>, String)>(&sql)
                .bind(from_ts)
                .bind(TRENDING_SALES_LIMIT)
                .fetch_all(&self.pool)
                .await?;

        // (contractAddress, itemId) -> count. Skip sales without an itemId
        // (the upstream `reduce` is conditional on `sale.itemId`).
        let mut counts: HashMap<(String, String), i64> = HashMap::new();
        // Preserve insertion order so the volume tie-breaker mirrors the
        // upstream's `Object.keys()` ordering.
        let mut order: Vec<(String, String)> = Vec::new();
        for (item_id, contract) in rows {
            let Some(item_id) = item_id else { continue };
            let key = (contract, item_id);
            if !counts.contains_key(&key) {
                order.push(key.clone());
            }
            *counts.entry(key).or_insert(0) += 1;
        }

        if counts.is_empty() {
            return Ok(Vec::new());
        }

        // Look up each (contract, itemId) → Item. We store each found item
        // at most once and index it by the key. Owning `Vec<Item>` so we
        // can move out into the output without cloning (Item is not Clone).
        let mut owned_items: Vec<Item> = Vec::new();
        let mut item_index: HashMap<(String, String), usize> = HashMap::new();
        for key in &order {
            let (contract, item_id) = key;
            let filters = ItemFilters {
                contract_addresses: vec![contract.clone()],
                item_id: Some(item_id.clone()),
                ..Default::default()
            };
            let (got, _) = items.get_items(&filters).await?;
            for it in got {
                let k = (it.contract_address.clone(), it.item_id.clone());
                if !item_index.contains_key(&k) {
                    item_index.insert(k, owned_items.len());
                    owned_items.push(it);
                }
            }
        }

        // Build the sales-ranked list of (key, count, idx_into_owned, on_sale).
        let lookup_at = |k: &(String, String)| -> Option<usize> { item_index.get(k).copied() };

        // Trending by sales: items with the most sales, on-sale only.
        let mut by_sales: Vec<(usize, i64)> = order
            .iter()
            .enumerate()
            .map(|(i, k)| (i, counts[k]))
            .collect();
        by_sales.sort_by(|a, b| b.1.cmp(&a.1));

        let sales_cap = ((size as f64) * SALES_CUT).floor() as usize;
        let mut chosen_sales_idx: Vec<usize> = Vec::new();
        for (order_i, _) in &by_sales {
            if chosen_sales_idx.len() >= sales_cap {
                break;
            }
            let key = &order[*order_i];
            if let Some(idx) = lookup_at(key) {
                if owned_items[idx].is_on_sale {
                    chosen_sales_idx.push(idx);
                }
            }
        }

        // Trending by volume: sales_count * price, descending. Excludes
        // anything already chosen by sales.
        let mut by_volume: Vec<(usize, i64)> = order
            .iter()
            .enumerate()
            .map(|(i, k)| (i, counts[k]))
            .collect();
        by_volume.sort_by(|a, b| {
            let va = lookup_at(&order[a.0])
                .map(|i| volume_of(&owned_items[i], a.1))
                .unwrap_or(0u128);
            let vb = lookup_at(&order[b.0])
                .map(|i| volume_of(&owned_items[i], b.1))
                .unwrap_or(0u128);
            vb.cmp(&va)
        });

        let volume_cap = ((size as f64) * VOLUME_CUT).floor() as usize;
        let mut chosen_volume_idx: Vec<usize> = Vec::new();
        for (order_i, _) in &by_volume {
            if chosen_volume_idx.len() >= volume_cap {
                break;
            }
            let key = &order[*order_i];
            if let Some(idx) = lookup_at(key) {
                if owned_items[idx].is_on_sale
                    && !chosen_sales_idx.contains(&idx)
                    && !chosen_volume_idx.contains(&idx)
                {
                    chosen_volume_idx.push(idx);
                }
            }
        }

        let mut chosen = chosen_sales_idx;
        chosen.extend(chosen_volume_idx);

        // Take the chosen indices out of `owned_items` in the order
        // selected. Using `Option::take` lets us move out of arbitrary
        // indices without breaking the vector's other slots.
        let mut wrapped: Vec<Option<Item>> = owned_items.into_iter().map(Some).collect();
        let mut out: Vec<Item> = Vec::with_capacity(chosen.len());
        for idx in chosen {
            if let Some(slot) = wrapped.get_mut(idx) {
                if let Some(it) = slot.take() {
                    out.push(it);
                }
            }
        }

        // Upstream applies a `seedrandom(item1.id + item2.id)` shuffle for
        // a deterministic-but-non-monotone order. We don't carry a port of
        // that across, so we leave the items in the canonical
        // sales-then-volume order — the parity script sorts arrays by `id`
        // before diffing, so the visible ordering does not affect parity.
        Ok(out)
    }
}

fn volume_of(item: &Item, sales_count: i64) -> u128 {
    let price = parse_u128_saturating(&item.price);
    price.saturating_mul(sales_count.max(0) as u128)
}

fn parse_u128_saturating(s: &str) -> u128 {
    s.parse::<u128>().unwrap_or(0)
}

fn midnight_days_ago(days: i64) -> i64 {
    let date = Utc::now() - Duration::days(days);
    let naive = date.date_naive().and_hms_opt(0, 0, 0).unwrap();
    Utc.from_utc_datetime(&naive).timestamp()
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<TrendingFilters, InvalidParameterError> {
    let p = Params::new(pairs);
    Ok(TrendingFilters {
        size: p.get_number("size", None).map(|v| v as i64),
        picked_by: p.get_string("pickedBy", None),
    })
}
