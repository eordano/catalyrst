use super::super::types::{csv, finite_i64, parse_shop_filters, ShopCatalogFilters};
use crate::http::params::Params;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedSource {
    Native,
    Legacy,
}

pub const UNIFIED_SOURCE_VALUES: &[&str] = &["native", "legacy"];

impl UnifiedSource {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "native" => Some(Self::Native),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Legacy => "legacy",
        }
    }
}

/// How the buyer acquires the item -- a SEPARATE question from how it is priced
/// (`UnifiedSource` answers "how is it PRICED", nothing else).
///
/// - `Trade`: an offchain-marketplace signed order, bought with `accept([trade])`.
/// - `Store`: a CollectionStore mint, bought with `CollectionStore.buy(...)`.
///   Not a listing at all: no order, no signature, and the supply is finite.
///
/// These two facts used to coincide -- everything MANA-priced was a legacy
/// trade -- so one enum covered both. CollectionStore mints break the
/// coincidence (MANA-priced AND not a trade), and collapsing them back into
/// `source` would silently change the meaning of every existing
/// `source == "legacy"` check. It also drives the buy path and the failure
/// modes the client has to surface: a store buy re-validates the price
/// on-chain (so it can revert on a price move) and can sell out between
/// browse and checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedAcquisition {
    Trade,
    Store,
}

impl UnifiedAcquisition {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Store => "store",
        }
    }
}

/// `Primary`: minted straight from a collection (`public_item_order`). `Secondary`: any resale. Omitted keeps both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopListingType {
    Primary,
    Secondary,
}

pub const SHOP_LISTING_TYPE_VALUES: &[&str] = &["primary", "secondary"];

impl ShopListingType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "primary" => Some(Self::Primary),
            "secondary" => Some(Self::Secondary),
            _ => None,
        }
    }
}

/// `Listing` (default): one row per open trade (the PDP resale view). `Item`: one row per item with a per-item `listingCount` (the shop browse feed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnifiedGroupBy {
    #[default]
    Listing,
    Item,
}

pub const UNIFIED_GROUP_BY_VALUES: &[&str] = &["listing", "item"];

impl UnifiedGroupBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "listing" => Some(Self::Listing),
            "item" => Some(Self::Item),
            _ => None,
        }
    }
}

/// Unknown or absent `groupBy` values fall back to the per-listing feed.
pub fn parse_unified_group_by(pairs: &[(String, String)]) -> UnifiedGroupBy {
    Params::new(pairs)
        .get_value("groupBy", UNIFIED_GROUP_BY_VALUES, None)
        .as_deref()
        .and_then(UnifiedGroupBy::parse)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default)]
pub struct UnifiedCatalogFilters {
    pub base: ShopCatalogFilters,
    pub source: Option<UnifiedSource>,
    /// Lets a caller hide resales server-side: this feed is paginated and reports a
    /// total, so dropping rows client-side yields short pages and an overstated count.
    pub listing_type: Option<ShopListingType>,
}

pub const SHOP_GENDER_VALUES: &[&str] = &["male", "female", "unisex"];

/// `wearableGender` in either encoding a caller might reach for: this feed's
/// comma-separated lists (what `rarity` and `wearableCategory` take) and the
/// repeated `&wearableGender=male&wearableGender=female` form /v1/items takes,
/// which is where the param and its values come from. Reaching for the wrong
/// one is what silently returned an unfiltered page (#391). Anything outside
/// [`SHOP_GENDER_VALUES`] is dropped, so a typo leaves the feed unfiltered
/// rather than asking for a body shape no item declares.
fn parse_wearable_genders(p: &Params) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for gender in csv(p.get_string("wearableGender", None))
        .into_iter()
        .chain(p.get_list("wearableGender", SHOP_GENDER_VALUES))
    {
        if SHOP_GENDER_VALUES.contains(&gender.as_str()) && !out.contains(&gender) {
            out.push(gender);
        }
    }
    out
}

/// The body shapes an item must DECLARE to satisfy a `wearableGender` request.
/// `unisex` asks for both, so it is the same request as male + female -- which
/// is exactly the set the response labels `unisex`. Mirrors the mapping
/// /v1/items uses; per-module copy, as ports/nfts and ports/items each keep.
pub fn body_shapes_for_genders(genders: &[String]) -> Option<Vec<String>> {
    let has_unisex = genders.iter().any(|g| g == "unisex");
    let has_male = has_unisex || genders.iter().any(|g| g == "male");
    let has_female = has_unisex || genders.iter().any(|g| g == "female");
    let mut out = Vec::new();
    if has_male {
        out.push("BaseMale".to_string());
    }
    if has_female {
        out.push("BaseFemale".to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub fn parse_unified_filters(pairs: &[(String, String)]) -> UnifiedCatalogFilters {
    let p = Params::new(pairs);
    let mut base = parse_shop_filters(pairs);
    base.wearable_genders = parse_wearable_genders(&p);
    UnifiedCatalogFilters {
        base,
        source: p
            .get_value("source", UNIFIED_SOURCE_VALUES, None)
            .as_deref()
            .and_then(UnifiedSource::parse),
        listing_type: p
            .get_value("listingType", SHOP_LISTING_TYPE_VALUES, None)
            .as_deref()
            .and_then(ShopListingType::parse),
    }
}

/// The parsed trending request: the row count, the look-back window, and the
/// NARROWED set of unified filters the ranking runs over. Only the fields
/// upstream's trending handler reads are parsed -- category, rarity,
/// wearableCategory, listingType, source, includeSocialEmotes -- so passing a
/// browse-only param (creator, search, a sort, a page) has no effect: the ranking
/// IS the sort here and a rail has no pages.
pub struct TrendingRequest {
    pub first: Option<i64>,
    pub days: Option<i64>,
    pub filters: UnifiedCatalogFilters,
}

pub fn parse_trending_filters(pairs: &[(String, String)]) -> TrendingRequest {
    let p = Params::new(pairs);
    let base = ShopCatalogFilters {
        category: p.get_string("category", None),
        rarities: csv(p.get_string("rarity", None)),
        wearable_categories: csv(p.get_string("wearableCategory", None)),
        include_social_emotes: p.get_string("includeSocialEmotes", None).as_deref()
            != Some("false"),
        ..Default::default()
    };
    TrendingRequest {
        first: finite_i64(p.get_number("first", None)),
        days: finite_i64(p.get_number("days", None)),
        filters: UnifiedCatalogFilters {
            base,
            source: p
                .get_value("source", UNIFIED_SOURCE_VALUES, None)
                .as_deref()
                .and_then(UnifiedSource::parse),
            listing_type: p
                .get_value("listingType", SHOP_LISTING_TYPE_VALUES, None)
                .as_deref()
                .and_then(ShopListingType::parse),
        },
    }
}
