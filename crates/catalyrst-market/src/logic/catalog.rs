use crate::dcl_schemas::{Network, NftCategory};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::ports::catalog::{CatalogFilters, CatalogSortBy, CatalogSortDirection};

const DEFAULT_PAGE_SIZE: i64 = 20;

const CATEGORY_VALUES: &[&str] = &["parcel", "estate", "wearable", "ens", "emote"];

const WEARABLE_CATEGORY_VALUES: &[&str] = &[
    "eyebrows",
    "eyes",
    "facial_hair",
    "hair",
    "body_shape",
    "mouth",
    "upper_body",
    "lower_body",
    "feet",
    "earring",
    "eyewear",
    "hat",
    "helmet",
    "mask",
    "tiara",
    "top_head",
    "skin",
    "hands_wear",
];

const EMOTE_CATEGORY_VALUES: &[&str] = &[
    "dance",
    "stunt",
    "greetings",
    "fun",
    "poses",
    "reactions",
    "horror",
    "miscellaneous",
];

const RARITY_VALUES: &[&str] = &[
    "unique",
    "mythic",
    "exotic",
    "legendary",
    "epic",
    "rare",
    "uncommon",
    "common",
];

const GENDER_VALUES: &[&str] = &["male", "female", "unisex"];

const EMOTE_PLAY_MODE_VALUES: &[&str] = &["simple", "loop"];

const EMOTE_OUTCOME_VALUES: &[&str] = &["so", "mo", "ro"];

const NETWORK_VALUES: &[&str] = &["ETHEREUM", "MATIC"];

const SORT_BY_VALUES: &[&str] = &[
    "newest",
    "recently_sold",
    "cheapest",
    "most_expensive",
    "recently_listed",
    "suggested",
];

const SORT_DIRECTION_VALUES: &[&str] = &["asc", "desc"];

pub fn parse_catalog_filters(
    pairs: &[(String, String)],
    is_v2: bool,
) -> Result<CatalogFilters, ApiError> {
    let p = Params::new(pairs);
    let _ = is_v2;

    let only_listing = p.get_boolean("onlyListing");
    let only_minting = p.get_boolean("onlyMinting");

    let sort_by = p
        .get_value("sortBy", SORT_BY_VALUES, Some("cheapest"))
        .and_then(|s| CatalogSortBy::parse(&s));
    let sort_direction = p
        .get_value("sortDirection", SORT_DIRECTION_VALUES, Some("asc"))
        .and_then(|s| CatalogSortDirection::parse(&s));

    let first = p
        .get_number("first", Some(DEFAULT_PAGE_SIZE as f64))
        .map(|n| n as i64);
    let skip = p.get_number("skip", Some(0.0)).map(|n| n as i64);

    let category = p
        .get_value("category", CATEGORY_VALUES, None)
        .and_then(parse_category);

    let creator = p.get_list("creator", &[]);

    let is_sold_out = p.get_boolean("isSoldOut");

    let is_on_sale = if p.get_boolean("isOnSale") {
        Some(p.get_string("isOnSale", None).as_deref() == Some("true"))
    } else {
        None
    };

    let search = p.get_string("search", None);

    let is_wearable_head = p.get_boolean("isWearableHead");
    let is_wearable_accessory = p.get_boolean("isWearableAccessory");
    let is_wearable_smart = p.get_boolean("isWearableSmart");

    let wearable_category = p.get_value("wearableCategory", WEARABLE_CATEGORY_VALUES, None);
    let rarities = p.get_list("rarity", RARITY_VALUES);
    let wearable_genders = p.get_list("wearableGender", GENDER_VALUES);
    let emote_category = p.get_value("emoteCategory", EMOTE_CATEGORY_VALUES, None);
    let emote_genders = p.get_list("emoteGender", GENDER_VALUES);
    let emote_play_mode = p.get_list("emotePlayMode", EMOTE_PLAY_MODE_VALUES);
    let emote_has_geometry = p.get_boolean("emoteHasGeometry");
    let emote_has_sound = p.get_boolean("emoteHasSound");
    let emote_outcome_type = p.get_value("emoteOutcomeType", EMOTE_OUTCOME_VALUES, None);

    let contract_addresses = p.get_address_list("contractAddress", false);
    let item_id = p.get_string("itemId", None);

    let network = p
        .get_value("network", NETWORK_VALUES, None)
        .and_then(parse_network);

    let max_price = p
        .get_string("maxPrice", None)
        .filter(|s| !s.trim().is_empty());
    let min_price = p
        .get_string("minPrice", None)
        .filter(|s| !s.trim().is_empty());

    let urns = p.get_list("urn", &[]);
    let ids = p.get_list("id", &[]);

    let include_social_emotes =
        Some(p.get_string("includeSocialEmotes", None).as_deref() != Some("false"));

    Ok(CatalogFilters {
        first,
        skip,
        sort_by,
        sort_direction,
        only_listing,
        only_minting,
        category,
        creator,
        is_sold_out,
        is_on_sale,
        search,
        is_wearable_head,
        is_wearable_accessory,
        is_wearable_smart,
        wearable_category,
        rarities,
        wearable_genders,
        emote_category,
        emote_genders,
        emote_play_mode,
        emote_has_geometry,
        emote_has_sound,
        emote_outcome_type,
        contract_addresses,
        item_id,
        network,
        max_price,
        min_price,
        urns,
        ids,
        picked_by: None,
        include_social_emotes,
    })
}

fn parse_category(s: String) -> Option<NftCategory> {
    Some(match s.as_str() {
        "parcel" => NftCategory::Parcel,
        "estate" => NftCategory::Estate,
        "wearable" => NftCategory::Wearable,
        "ens" => NftCategory::Ens,
        "emote" => NftCategory::Emote,
        _ => return None,
    })
}

fn parse_network(s: String) -> Option<Network> {
    Some(match s.as_str() {
        "ETHEREUM" => Network::Ethereum,
        "MATIC" => Network::Matic,
        _ => return None,
    })
}
