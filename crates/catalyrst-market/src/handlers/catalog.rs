use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::ser::{Serialize, SerializeSeq, SerializeStruct, Serializer};

use crate::auth_chain;
use crate::http::response::ApiError;
use crate::logic::catalog::parse_catalog_filters;
use crate::ports::catalog::{CatalogItem, PickStats};
use crate::AppState;

/// The catalog page held as the shared cache `Arc` plus this request's per-item picks, serialized
/// as `{"data": [...], "total": N}` -- byte-identical to `DataTotal<CatalogItem>`. Only the items
/// that actually carry picks are cloned, so a cache hit no longer deep-clones the whole page.
pub struct CatalogPage {
    page: Arc<(Vec<CatalogItem>, i64)>,
    picks: Vec<Option<PickStats>>,
}

impl Serialize for CatalogPage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Data<'a>(&'a CatalogPage);

        impl Serialize for Data<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let items = &self.0.page.0;
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for (item, pick) in items.iter().zip(&self.0.picks) {
                    if pick.is_none() && item.picks.is_none() {
                        seq.serialize_element(item)?;
                    } else {
                        let mut owned = item.clone();
                        owned.picks = pick.clone();
                        seq.serialize_element(&owned)?;
                    }
                }
                seq.end()
            }
        }

        let mut s = serializer.serialize_struct("DataTotal", 2)?;
        s.serialize_field("data", &Data(self))?;
        s.serialize_field("total", &self.page.1)?;
        s.end()
    }
}

pub async fn get_catalog_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<CatalogPage>, ApiError> {
    get_catalog_inner(state, headers, pairs, false).await
}

pub async fn get_catalog_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<CatalogPage>, ApiError> {
    get_catalog_inner(state, headers, pairs, true).await
}

async fn get_catalog_inner(
    state: AppState,
    headers: HeaderMap,
    pairs: Vec<(String, String)>,
    is_v2: bool,
) -> Result<Json<CatalogPage>, ApiError> {
    let filters = parse_catalog_filters(&pairs, is_v2)?;

    let picked_by = auth_chain::optional_signer(
        &headers,
        "get",
        if is_v2 { "/v2/catalog" } else { "/v1/catalog" },
    )
    .await?;

    let search_id = headers
        .get("X-Search-Uuid")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let anon_id = headers
        .get("X-Anonymous-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let page = state
        .catalog
        .fetch(filters, &search_id, &anon_id, is_v2)
        .await?;

    let ids: Vec<String> = page.0.iter().map(|item| item.id.clone()).collect();
    let mut by_id: HashMap<String, PickStats> = state
        .lists
        .get_picks_stats(&ids, picked_by.as_deref())
        .await?
        .into_iter()
        .map(|stats| (stats.item_id.clone(), stats))
        .collect();
    let picks = page.0.iter().map(|item| by_id.remove(&item.id)).collect();

    Ok(Json(CatalogPage { page, picks }))
}

#[cfg(test)]
mod catalog_page_shape_tests {
    use super::CatalogPage;
    use crate::dcl_schemas::{ChainId, Network};
    use crate::http::response::DataTotal;
    use crate::ports::catalog::{CatalogItem, ItemData, PickStats, WearableData};
    use std::sync::Arc;

    fn wearable_item() -> CatalogItem {
        CatalogItem {
            id: "0xc-1".into(),
            beneficiary: Some("0xbeef".into()),
            item_id: "1".into(),
            name: "hat".into(),
            thumbnail: "thumb".into(),
            url: "/contracts/0xc/items/1".into(),
            urn: "urn:x".into(),
            category: "wearable",
            contract_address: "0xc".into(),
            rarity: "epic".into(),
            available: 2,
            is_on_sale: true,
            trade_id: Some("t-1".into()),
            creator: "0xcr".into(),
            data: ItemData::Wearable {
                wearable: WearableData {
                    description: Some("d".into()),
                    category: Some("hat".into()),
                    body_shapes: vec!["BaseMale".into()],
                    rarity: "epic".into(),
                    is_smart: false,
                },
            },
            network: Network::Ethereum,
            chain_id: ChainId::EthereumMainnet,
            price: "10".into(),
            created_at: 1,
            updated_at: 2,
            reviewed_at: 3,
            first_listed_at: Some(99),
            sold_at: 4,
            min_price: Some("7".into()),
            max_listing_price: Some("20".into()),
            min_listing_price: Some("7".into()),
            listings: Some(3),
            owners: Some(5),
            picks: None,
        }
    }

    fn emote_item() -> CatalogItem {
        CatalogItem {
            id: "0xe-3".into(),
            beneficiary: None,
            item_id: "3".into(),
            name: "dance".into(),
            thumbnail: "et".into(),
            url: "/contracts/0xe/items/3".into(),
            urn: "urn:e".into(),
            category: "emote",
            contract_address: "0xe".into(),
            rarity: "rare".into(),
            available: 1,
            is_on_sale: false,
            trade_id: None,
            creator: "0xec".into(),
            data: ItemData::Emote {
                emote: serde_json::json!({
                    "description": "edesc",
                    "category": "dance",
                    "bodyShapes": ["BaseFemale"],
                    "rarity": "rare",
                    "loop": true,
                    "hasGeometry": false,
                    "hasSound": true,
                    "outcomeType": "win"
                }),
            },
            network: Network::Matic,
            chain_id: ChainId::MaticMainnet,
            price: "0".into(),
            created_at: 10,
            updated_at: 20,
            reviewed_at: 30,
            first_listed_at: None,
            sold_at: 40,
            min_price: None,
            max_listing_price: None,
            min_listing_price: None,
            listings: None,
            owners: None,
            picks: None,
        }
    }

    fn stats() -> PickStats {
        PickStats {
            count: 8,
            item_id: "0xc-1".into(),
            picked_by_user: Some(true),
        }
    }

    #[test]
    fn catalog_page_serializes_byte_identical_to_data_total() {
        for picks in [
            vec![None, None],
            vec![Some(stats()), None],
            vec![None, Some(stats())],
        ] {
            let items = vec![wearable_item(), emote_item()];

            let mut expected = items.clone();
            for (item, pick) in expected.iter_mut().zip(&picks) {
                item.picks = pick.clone();
            }
            let expected = serde_json::to_string(&DataTotal {
                data: expected,
                total: 7,
            })
            .unwrap();

            let got = serde_json::to_string(&CatalogPage {
                page: Arc::new((items, 7)),
                picks,
            })
            .unwrap();

            assert_eq!(got, expected);
        }
    }
}
