use crate::dcl_schemas::{get_db_networks, Network, NftCategory};
use crate::http::params::Params;
use crate::http::response::ApiError;

use super::types::{NftFilters, NftSortBy};

pub fn get_db_networks_for(network: &str) -> Vec<String> {
    let net = match network {
        "ETHEREUM" => Some(Network::Ethereum),
        "MATIC" | "POLYGON" => Some(Network::Matic),
        _ => None,
    };
    match net {
        Some(n) => get_db_networks(n).into_iter().map(String::from).collect(),
        None => Vec::new(),
    }
}

pub fn parse_filters(pairs: &[(String, String)]) -> Result<NftFilters, ApiError> {
    let p = Params::new(pairs);

    let nft_categories = &["parcel", "estate", "wearable", "ens", "emote"];
    let category = p
        .get_value("category", nft_categories, None)
        .map(|s| match s.as_str() {
            "parcel" => NftCategory::Parcel,
            "estate" => NftCategory::Estate,
            "wearable" => NftCategory::Wearable,
            "ens" => NftCategory::Ens,
            "emote" => NftCategory::Emote,
            _ => unreachable!(),
        });

    let networks = &["ETHEREUM", "MATIC"];
    let network = p
        .get_value("network", networks, None)
        .map(|s| match s.as_str() {
            "ETHEREUM" => Network::Ethereum,
            "MATIC" => Network::Matic,
            _ => unreachable!(),
        });

    let sort_by_allow = &[
        "name",
        "newest",
        "recently_listed",
        "recently_sold",
        "cheapest_parcel",
    ];
    let sort_by = p
        .get_value("sortBy", sort_by_allow, None)
        .and_then(|s| NftSortBy::parse_str(&s));

    let rental_days: Vec<i64> = p
        .get_list("rentalDays", &[])
        .into_iter()
        .filter_map(|d| d.parse::<i64>().ok())
        .collect();

    Ok(NftFilters {
        first: p.get_number("first", None).map(|f| f as i64),
        skip: p.get_number("skip", None).map(|f| f as i64),
        sort_by,
        category,
        owner: p.get_address("owner", false, None),
        tenant: p
            .get_address("tenant", false, None)
            .map(|s| s.to_lowercase()),
        is_on_sale: parse_optional_bool(&p, "isOnSale"),
        is_on_rent: p.get_boolean("isOnRent"),
        search: p.get_string("search", None),
        is_land: p.get_boolean("isLand"),
        is_wearable_head: p.get_boolean("isWearableHead"),
        is_wearable_accessory: p.get_boolean("isWearableAccessory"),
        is_wearable_smart: p.get_boolean("isWearableSmart"),
        wearable_category: p.get_string("wearableCategory", None),
        wearable_genders: p.get_list("wearableGender", &[]),
        emote_category: p.get_string("emoteCategory", None),
        emote_genders: p.get_list("emoteGender", &[]),
        emote_play_mode: p.get_list("emotePlayMode", &[]),
        contract_addresses: p.get_address_list("contractAddress", false),
        creator: p.get_list("creator", &[]),
        token_id: p.get_string("tokenId", None),
        item_rarities: p.get_list("itemRarity", &[]),
        item_id: p.get_string("itemId", None),
        network,
        rental_status: p.get_list("rentalStatus", &[]),
        adjacent_to_road: p.get_boolean("adjacentToRoad"),
        min_distance_to_plaza: p.get_number("minDistanceToPlaza", None),
        max_distance_to_plaza: p.get_number("maxDistanceToPlaza", None),
        min_estate_size: p.get_number("minEstateSize", None),
        max_estate_size: p.get_number("maxEstateSize", None),
        min_price: p
            .get_string("minPrice", None)
            .filter(|s| !s.trim().is_empty()),
        max_price: p
            .get_string("maxPrice", None)
            .filter(|s| !s.trim().is_empty()),
        emote_has_geometry: p.get_boolean("emoteHasGeometry"),
        emote_has_sound: p.get_boolean("emoteHasSound"),
        emote_outcome_type: p.get_string("emoteOutcomeType", None),
        rental_days,
        ids: p.get_list("id", &[]),
        banned_names: Vec::new(),
        include_social_emotes: Some(
            p.get_string("includeSocialEmotes", None).as_deref() != Some("false"),
        ),
    })
}

fn parse_optional_bool(p: &Params, key: &str) -> Option<bool> {
    if p.get_boolean(key) {
        p.get_string(key, None).map(|s| s == "true")
    } else {
        None
    }
}
