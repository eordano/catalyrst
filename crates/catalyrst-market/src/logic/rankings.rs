use std::collections::HashMap;

use serde::Serialize;

use crate::logic::numeric::bn_add;

#[derive(Debug, Clone, Serialize)]
pub struct ItemRank {
    pub id: String,
    pub sales: i64,
    pub volume: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatorRank {
    pub id: String,
    pub sales: i64,
    pub earned: String,
    #[serde(rename = "uniqueCollectors")]
    pub unique_collectors: i64,
    pub collections: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectorRank {
    pub id: String,
    pub purchases: i64,
    pub spent: String,
    #[serde(rename = "uniqueAndMythicItems")]
    pub unique_and_mythic_items: i64,
    #[serde(rename = "creatorsSupported")]
    pub creators_supported: i64,
}

#[derive(Debug, Clone)]
pub struct ItemsDayDataFragment {
    pub id: String,
    pub sales: i64,
    pub volume: String,
}

#[derive(Debug, Clone)]
pub struct CreatorsDayDataFragment {
    pub id: String,
    pub sales: i64,
    pub earned: String,
    pub unique_collections_sales: i64,
    pub unique_collectors_total: i64,
}

#[derive(Debug, Clone)]
pub struct CollectorsDayDataFragment {
    pub id: String,
    pub purchases: i64,
    pub spent: String,
    pub unique_and_mythic_items: i64,
    pub creators_supported_total: i64,
}

pub fn get_unique_items_from_items_day_data(
    fragments: Vec<ItemsDayDataFragment>,
    from: i64,
) -> Vec<ItemRank> {
    let mut acc: HashMap<String, ItemRank> = HashMap::new();
    for f in fragments {
        let item_id = if f.id.contains('-') {
            f.id.clone()
        } else if from == 0 {
            f.id.clone()
        } else {
            f.id[f.id.find('-').unwrap_or(0) + 1..].to_string()
        };
        let entry = acc.entry(item_id.clone()).or_insert_with(|| ItemRank {
            id: item_id.clone(),
            sales: 0,
            volume: "0".to_string(),
        });
        entry.sales += f.sales;
        entry.volume = bn_add(&entry.volume, &f.volume);
    }
    acc.into_values().collect()
}

pub fn get_unique_creators_from_creators_day_data(
    fragments: Vec<CreatorsDayDataFragment>,
) -> Vec<CreatorRank> {
    let mut acc: HashMap<String, CreatorsDayDataFragment> = HashMap::new();
    for f in fragments {
        let address = if let Some(i) = f.id.find('-') {
            f.id[i + 1..].to_string()
        } else {
            f.id.clone()
        };
        let entry = acc
            .entry(address.clone())
            .or_insert_with(|| CreatorsDayDataFragment {
                id: address.clone(),
                sales: 0,
                earned: "0".to_string(),
                unique_collections_sales: 0,
                unique_collectors_total: 0,
            });
        entry.sales += f.sales;
        entry.earned = bn_add(&entry.earned, &f.earned);
        entry.unique_collections_sales = entry
            .unique_collections_sales
            .max(f.unique_collections_sales);
        entry.unique_collectors_total =
            entry.unique_collectors_total.max(f.unique_collectors_total);
    }
    acc.into_values()
        .map(|f| CreatorRank {
            id: f.id,
            sales: f.sales,
            earned: f.earned,
            unique_collectors: f.unique_collectors_total,
            collections: f.unique_collections_sales,
        })
        .collect()
}

pub fn get_unique_collectors_from_collectors_day_data(
    fragments: Vec<CollectorsDayDataFragment>,
) -> Vec<CollectorRank> {
    let mut acc: HashMap<String, CollectorsDayDataFragment> = HashMap::new();
    for f in fragments {
        let address = if let Some(i) = f.id.find('-') {
            f.id[i + 1..].to_string()
        } else {
            f.id.clone()
        };
        let entry = acc
            .entry(address.clone())
            .or_insert_with(|| CollectorsDayDataFragment {
                id: address.clone(),
                purchases: 0,
                spent: "0".to_string(),
                unique_and_mythic_items: 0,
                creators_supported_total: 0,
            });
        entry.purchases += f.purchases;
        entry.spent = bn_add(&entry.spent, &f.spent);
        entry.unique_and_mythic_items =
            entry.unique_and_mythic_items.max(f.unique_and_mythic_items);
        entry.creators_supported_total = entry
            .creators_supported_total
            .max(f.creators_supported_total);
    }
    acc.into_values()
        .map(|f| CollectorRank {
            id: f.id,
            purchases: f.purchases,
            spent: f.spent,
            unique_and_mythic_items: f.unique_and_mythic_items,
            creators_supported: f.creators_supported_total,
        })
        .collect()
}
