use super::super::types::{
    csv, finite_i64, parse_listing_type, parse_shop_filters, ShopCatalogFilters,
};
use crate::http::params::{is_address, Params};

pub use super::super::types::{ShopListingType, SHOP_LISTING_TYPE_VALUES};

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

/// How the buyer acquires the item -- a SEPARATE question from how it is priced, which is all
/// `UnifiedSource` answers.
///
/// - `Trade`: an offchain-marketplace signed order, bought with `accept([trade])`.
/// - `Store`: a CollectionStore mint, bought with `CollectionStore.buy(...)`. Not a listing
///   at all: no order, no signature, and the supply is finite.
///
/// The two facts used to coincide -- everything MANA-priced was a legacy trade -- so one enum
/// covered both. CollectionStore mints break that, and collapsing them back into `source`
/// would silently change the meaning of every existing `source == "legacy"` check. It also
/// drives the buy path and the failure modes the client must surface: a store buy re-validates
/// the price on-chain (so it can revert on a price move) and can sell out between browse and
/// checkout.
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
    /// A SET of collections, where `base.contract_address` is one. Only the unified feed reads
    /// it, which is why it does not live on [`ShopCatalogFilters`].
    ///
    /// `None` and `Some(vec![])` are NOT the same: `None` is no collection filter, `Some(vec![])`
    /// is a set the caller named that resolved to nothing and must yield an empty page. Folding
    /// the two together would serve the whole catalogue to a caller whose filter failed to
    /// resolve, which reads as a working event rather than as an error.
    pub contract_addresses: Option<Vec<String>>,
    /// Whether the LEGACY (classic MANA-priced) branch may contribute SECONDARY listings --
    /// resales. False keeps that branch primary-only, which is this feed's pre-existing
    /// answer; only an explicit `includeLegacySecondary=true` may change it.
    ///
    /// It exists because resale LISTING lives in the classic Marketplace, so a copy somebody
    /// put up for sale is a `public_nft_order` priced in MANA -- exactly the combination the
    /// legacy branch excluded. The native (USD-pegged) branch has always carried resales.
    ///
    /// Orthogonal to `base.listing_type`: that narrows the result to one kind, this decides
    /// whether one SOURCE may contribute resales at all. Asking for `listingType=secondary`
    /// without it yields native resales only.
    pub include_legacy_secondary: bool,
}

/// What the related rail reads beyond its anchor. The rail is drawn from the same universe as
/// the grid, so it takes the same opt-in -- a rail that included a row the grid excludes would
/// contradict the page around it. `listing_type` is the one people forget here: the opt-in
/// governs only the LEGACY branch, while native resales reach this rail unconditionally.
#[derive(Debug, Clone, Default)]
pub struct RelatedItemsFilters {
    pub include_legacy_secondary: bool,
    pub listing_type: Option<ShopListingType>,
}

pub fn parse_related_filters(pairs: &[(String, String)]) -> RelatedItemsFilters {
    let p = Params::new(pairs);
    RelatedItemsFilters {
        include_legacy_secondary: parse_include_legacy_secondary(&p),
        listing_type: parse_listing_type(&p),
    }
}

pub const SHOP_GENDER_VALUES: &[&str] = &["male", "female", "unisex"];

/// Accepts either encoding a caller might reach for: this feed's comma-separated lists (what
/// `rarity` and `wearableCategory` take) and the repeated
/// `&wearableGender=male&wearableGender=female` form /v1/items takes. Reaching for the wrong
/// one is what silently returned an unfiltered page (#391). Anything outside
/// [`SHOP_GENDER_VALUES`] is dropped, so a typo leaves the feed unfiltered rather than asking
/// for a body shape no item declares.
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

/// `unisex` asks for both, so it is the same request as male + female -- exactly the set the
/// response labels `unisex`. Mirrors the mapping /v1/items uses; a per-module copy, as
/// ports/nfts and ports/items each keep.
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

/// The collections the unified feed is restricted to, or `None` when the caller named none.
///
/// Takes either encoding, as `parse_wearable_genders` above does: the comma form
/// (`contractAddress=0xa,0xb`) and the repeated form, which `get_list` also reads as
/// `contractAddress[]`. The comma form is what a seasonal event needs -- it selects its items by
/// tagging whole collections and routinely names dozens.
///
/// A blank value reads as ABSENT, which is what it has always meant here. A value that is present
/// but is not an address yields an empty set, i.e. an empty page -- which is also what the
/// singular filter has always produced for a non-address, since it matched no row.
fn contract_address_list(p: &Params) -> Option<Vec<String>> {
    let named: Vec<String> = p
        .get_list("contractAddress", &[])
        .iter()
        .flat_map(|v| v.split(','))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if named.is_empty() {
        return None;
    }
    Some(
        named
            .into_iter()
            .filter(|v| is_address(v))
            .map(|v| v.to_lowercase())
            .collect(),
    )
}

pub fn parse_unified_filters(pairs: &[(String, String)]) -> UnifiedCatalogFilters {
    let p = Params::new(pairs);
    let mut base = parse_shop_filters(pairs);
    base.wearable_genders = parse_wearable_genders(&p);
    let contract_addresses = contract_address_list(&p);
    if contract_addresses.is_some() {
        base.contract_address = None;
    }
    UnifiedCatalogFilters {
        base,
        source: p
            .get_value("source", UNIFIED_SOURCE_VALUES, None)
            .as_deref()
            .and_then(UnifiedSource::parse),
        contract_addresses,
        include_legacy_secondary: parse_include_legacy_secondary(&p),
    }
}

/// Compared against the literal `true`, not read as a presence flag: this is an opt-in whose
/// default is the pre-existing feed, so an absent key, a `false` and a typo must all keep
/// today's response. (`includeSocialEmotes` compares against `false` for the mirror reason.)
fn parse_include_legacy_secondary(p: &Params) -> bool {
    p.get_string("includeLegacySecondary", None).as_deref() == Some("true")
}

/// `filters` is NARROWED to what upstream's trending handler reads -- category, rarity,
/// wearableCategory, listingType, source, includeSocialEmotes -- so a browse-only param
/// (creator, search, a sort, a page) has no effect: the ranking IS the sort and a rail has no
/// pages.
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
        listing_type: parse_listing_type(&p),
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
            contract_addresses: None,
            include_legacy_secondary: parse_include_legacy_secondary(&p),
        },
    }
}
