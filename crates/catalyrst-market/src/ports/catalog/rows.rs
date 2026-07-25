use serde_json::Value as JsonValue;

use crate::dcl_schemas::{
    ethereum_chain_id, polygon_chain_id, repoint_content_url, ChainId, Network,
};

use super::types::{CatalogItem, DbRow, ItemData, WearableData};
use super::{
    FRAGMENT_EMOTE_V1, FRAGMENT_SMART_WEARABLE_V1, FRAGMENT_WEARABLE_V1, FRAGMENT_WEARABLE_V2,
};

fn fix_thumbnail(thumbnail: &str, blockchain_id: &str) -> String {
    if thumbnail.is_empty() {
        return String::new();
    }

    let mut t = if matches!(polygon_chain_id(), ChainId::MaticAmoy)
        || matches!(ethereum_chain_id(), ChainId::EthereumSepolia)
    {
        thumbnail.replace(".org", ".zone")
    } else {
        thumbnail.to_string()
    };

    t = t.replace("polygon", "matic").replace("mainnet", "ethereum");

    if t.contains("ethereum") {
        return t;
    }

    let mut parts: Vec<String> = t.split(':').map(String::from).collect();
    if parts.len() <= 5 {
        return t;
    }
    if !parts[5].starts_with("0x") {
        let with_prefix = format!("0x{}", parts[5]);
        parts[5] = with_prefix.replace("/thumbnail", &format!(":{}/thumbnail", blockchain_id));
    }
    parts.join(":")
}

pub(super) fn from_db_row_to_catalog_item(
    row: DbRow,
    network_hint: Option<Network>,
) -> CatalogItem {
    let metadata = row.metadata.clone().unwrap_or(JsonValue::Null);
    let meta_obj = metadata.as_object();
    let get_str = |key: &str| -> Option<String> {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_str().map(String::from))
    };
    let get_bool = |key: &str| -> bool {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let get_string_array = |key: &str| -> Vec<String> {
        meta_obj
            .and_then(|m| m.get(key))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };

    let (name, category, data): (String, &'static str, ItemData) = match row.item_type.as_str() {
        FRAGMENT_WEARABLE_V1 | FRAGMENT_WEARABLE_V2 | FRAGMENT_SMART_WEARABLE_V1 => {
            let wearable = WearableData {
                description: get_str("description"),
                category: get_str("category"),
                body_shapes: get_string_array("body_shapes"),
                rarity: row.rarity.clone(),
                is_smart: row.item_type == FRAGMENT_SMART_WEARABLE_V1,
            };
            (
                get_str("name").unwrap_or_default(),
                "wearable",
                ItemData::Wearable { wearable },
            )
        }
        FRAGMENT_EMOTE_V1 => {
            let emote_category_lower = get_str("category").map(|s| s.to_lowercase());
            let emote_value = serde_json::json!({
                "description": get_str("description"),
                "category": emote_category_lower,
                "bodyShapes": get_string_array("body_shapes"),
                "rarity": row.rarity,
                "loop": get_bool("loop"),
                "hasGeometry": get_bool("has_geometry"),
                "hasSound": get_bool("has_sound"),
                "outcomeType": get_str("outcome_type"),
            });
            (
                get_str("name").unwrap_or_default(),
                "emote",
                ItemData::Emote { emote: emote_value },
            )
        }
        other => {
            tracing::warn!(item_type = %other, item_id = %row.id, "unknown item_type, defaulting to wearable");
            (
                String::new(),
                "wearable",
                ItemData::Wearable {
                    wearable: WearableData {
                        description: None,
                        category: None,
                        body_shapes: vec![],
                        rarity: row.rarity.clone(),
                        is_smart: false,
                    },
                },
            )
        }
    };

    let available_n = row.available.parse::<i64>().unwrap_or(0);
    let price = if available_n > 0 {
        if row.open_item_trade_id.is_some() && row.search_is_marketplace_v3_minter {
            row.open_item_trade_price
                .clone()
                .unwrap_or_else(|| "0".into())
        } else if row.search_is_store_minter {
            row.price.clone()
        } else {
            "0".into()
        }
    } else {
        "0".into()
    };

    let item_network_str = if !row.network.is_empty() {
        row.network.clone()
    } else {
        match network_hint {
            Some(Network::Ethereum) => "ETHEREUM".into(),
            _ => "POLYGON".into(),
        }
    };
    let (item_network, chain_id) = if item_network_str.eq_ignore_ascii_case("POLYGON")
        || item_network_str.eq_ignore_ascii_case("MATIC")
    {
        (Network::Matic, polygon_chain_id())
    } else {
        (Network::Ethereum, ethereum_chain_id())
    };

    let parse_i64_lossy = |s: &str| s.parse::<i64>().unwrap_or(0);

    CatalogItem {
        id: format!("{}-{}", row.collection_id, row.blockchain_id),
        beneficiary: row.beneficiary.clone(),
        item_id: row.blockchain_id.clone(),
        name,
        thumbnail: repoint_content_url(&fix_thumbnail(&row.image, &row.blockchain_id)),
        url: format!(
            "/contracts/{}/items/{}",
            row.collection_id, row.blockchain_id
        ),
        urn: row.urn.clone(),
        category,
        contract_address: row.collection_id.clone(),
        rarity: row.rarity.clone(),
        available: available_n,
        is_on_sale: (row.search_is_store_minter
            || (row.open_item_trade_id.is_some() && row.search_is_marketplace_v3_minter))
            && available_n > 0,
        creator: row.creator.clone(),
        data,
        network: item_network,
        chain_id,
        price,
        created_at: parse_i64_lossy(&row.created_at),
        updated_at: parse_i64_lossy(&row.updated_at),
        reviewed_at: parse_i64_lossy(&row.reviewed_at),
        first_listed_at: row.first_listed_at.as_deref().map(parse_i64_lossy),
        sold_at: parse_i64_lossy(&row.sold_at),
        min_price: row.min_price.clone(),
        max_listing_price: row.max_listing_price.clone(),
        min_listing_price: row.min_listing_price.clone(),
        listings: row.listings_count,
        owners: row.owners_count,
        picks: None,
    }
}
