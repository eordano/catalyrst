use crate::dcl_schemas::{ethereum_chain_id, polygon_chain_id, repoint_content_url, Network};
use crate::ports::items::fix_urn;

use super::types::{
    DbNft, EmoteData, EnsData, EstateData, Nft, NftData, ParcelData, ParcelEstate, WearableData,
};

pub fn from_db_nft_to_nft(d: &DbNft) -> Nft {
    let network_canonical = match d.network.as_deref() {
        Some("MATIC") | Some("POLYGON") => Network::Matic,
        _ => Network::Ethereum,
    };
    let chain_id = match d.network.as_deref() {
        Some("MATIC") | Some("POLYGON") => polygon_chain_id(),
        _ => ethereum_chain_id(),
    };

    let category = d.category.clone().unwrap_or_default();
    let contract = d.contract_address.clone().unwrap_or_default();
    let token_id = d.token_id.clone().unwrap_or_default();

    let data = build_nft_data(d, &category);

    Nft {
        active_order_id: None,
        category: category.clone(),
        chain_id,
        contract_address: contract.clone(),
        created_at: from_seconds_to_millis(d.created_at.unwrap_or(0)),
        data,
        id: format!("{}-{}", contract, token_id),
        image: repoint_content_url(&fix_urn(&d.image.clone().unwrap_or_default())),
        issued_id: d.issued_id.clone(),
        item_id: d.item_id.clone(),
        name: d.name.clone().unwrap_or_else(|| capitalize(&category)),
        network: network_canonical,
        open_rental_id: None,
        owner: d.owner.clone().unwrap_or_default(),
        token_id: token_id.clone(),
        sold_at: 0,
        updated_at: from_seconds_to_millis(d.updated_at.unwrap_or(0)),
        url: format!("/contracts/{}/tokens/{}", contract, token_id),
        urn: d.urn.as_ref().map(|u| fix_urn(u)),
    }
}

fn build_nft_data(d: &DbNft, category: &str) -> NftData {
    let rarity = d.rarity.clone().unwrap_or_default();
    let description = d.description.clone();

    match category {
        "wearable" => NftData::Wearable {
            wearable: WearableData {
                body_shapes: d.body_shapes.clone().unwrap_or_default(),
                category: d.wearable_category.clone().unwrap_or_default(),
                description: description.unwrap_or_default(),
                rarity,
                is_smart: d.item_type.as_deref() == Some("smart_wearable_v1"),
            },
        },
        "parcel" => NftData::Parcel {
            parcel: ParcelData {
                x: d.x.clone().unwrap_or_default(),
                y: d.y.clone().unwrap_or_default(),
                description,
                estate: d.parcel_estate_id.as_ref().map(|_| ParcelEstate {
                    name: d
                        .parcel_estate_name
                        .clone()
                        .unwrap_or_else(|| capitalize("estate")),
                    token_id: d.parcel_estate_token_id.clone().unwrap_or_default(),
                }),
            },
        },
        "ens" => NftData::Ens {
            ens: EnsData {
                subdomain: d.subdomain.clone().unwrap_or_default(),
            },
        },
        "estate" => NftData::Estate {
            estate: EstateData {
                size: d.size.unwrap_or(0) as i64,
                description,
                parcels: d
                    .estate_parcels
                    .as_ref()
                    .map(|j| j.0.clone())
                    .unwrap_or_default(),
            },
        },
        _ => NftData::Emote {
            emote: EmoteData {
                body_shapes: d.body_shapes.clone().unwrap_or_default(),
                category: d.emote_category.clone().unwrap_or_default(),
                description: description.unwrap_or_default(),
                rarity,
                r#loop: d.r#loop.unwrap_or(false),
                has_sound: d.has_sound.unwrap_or(false),
                has_geometry: d.has_geometry.unwrap_or(false),
                outcome_type: d.emote_outcome_type.clone(),
            },
        },
    }
}

fn from_seconds_to_millis(s: i64) -> i64 {
    s.saturating_mul(1000)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
