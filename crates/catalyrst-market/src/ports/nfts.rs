use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;

use crate::dcl_schemas::{
    ethereum_chain_id, get_db_networks, polygon_chain_id, ChainId, Network, NftCategory,
};
use crate::http::params::Params;
use crate::http::response::ApiError;
use crate::logic::sql_filters::{clamp_first, clamp_skip, where_from};
use crate::ports::items::{fix_urn, ItemType};
use crate::MARKETPLACE_SQUID_SCHEMA;

pub const MAX_ORDER_TIMESTAMP: i64 = 253_378_408_747_000;

/// The `nft.network` values to match for a trade's `network` string, mirroring
/// upstream `getDBNetworks(network)`. A trade's network is the `@dcl/schemas`
/// `Network` enum value ("ETHEREUM" / "MATIC"); MATIC also matches the squid
/// "POLYGON" alias. Unknown strings yield an empty set (no match), matching
/// upstream's fall-through `return []`.
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

#[derive(Debug, Clone, Default)]
pub struct NftFilters {
    pub first: Option<i64>,
    pub skip: Option<i64>,
    pub sort_by: Option<NftSortBy>,
    pub category: Option<NftCategory>,
    pub owner: Option<String>,
    pub tenant: Option<String>,
    pub is_on_sale: Option<bool>,
    pub is_on_rent: bool,
    pub search: Option<String>,
    pub is_land: bool,
    pub is_wearable_head: bool,
    pub is_wearable_accessory: bool,
    pub is_wearable_smart: bool,
    pub wearable_category: Option<String>,
    pub wearable_genders: Vec<String>,
    pub emote_category: Option<String>,
    pub emote_genders: Vec<String>,
    pub emote_play_mode: Vec<String>,
    pub contract_addresses: Vec<String>,
    pub creator: Vec<String>,
    pub token_id: Option<String>,
    pub item_rarities: Vec<String>,
    pub item_id: Option<String>,
    pub network: Option<Network>,
    pub rental_status: Vec<String>,
    pub adjacent_to_road: bool,
    pub min_distance_to_plaza: Option<f64>,
    pub max_distance_to_plaza: Option<f64>,
    pub min_estate_size: Option<f64>,
    pub max_estate_size: Option<f64>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
    pub emote_has_geometry: bool,
    pub emote_has_sound: bool,
    pub emote_outcome_type: Option<String>,
    pub rental_days: Vec<i64>,
    pub ids: Vec<String>,
    pub banned_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSortBy {
    Name,
    Newest,
    RecentlyListed,
    RecentlySold,
    CheapestParcel,
}

impl NftSortBy {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "name" => Some(Self::Name),
            "newest" => Some(Self::Newest),
            "recently_listed" => Some(Self::RecentlyListed),
            "recently_sold" => Some(Self::RecentlySold),
            "cheapest_parcel" => Some(Self::CheapestParcel),
            _ => None,
        }
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
    })
}

fn parse_optional_bool(p: &Params, key: &str) -> Option<bool> {
    if p.get_boolean(key) {
        p.get_string(key, None).map(|s| s == "true")
    } else {
        None
    }
}

pub struct NftErrors;
impl NftErrors {
    pub const INVALID_SEARCH_BY_TENANT_AND_OWNER: &'static str =
        "Owner or tenant can be set, but not both.";
    pub const INVALID_TOKEN_ID: &'static str = "Invalid token id, token ids must be numbers";
    pub const MISSING_CONTRACT_ADDRESS: &'static str =
        "NFTs can't be queried by token id if no contract address is provided";
}

#[derive(Debug, sqlx::FromRow)]
pub struct DbNft {
    pub id: String,
    pub count: i64,
    pub contract_address: Option<String>,
    pub token_id: Option<String>,
    pub network: Option<String>,
    pub created_at: Option<i64>,
    pub url: Option<String>,
    pub updated_at: Option<i64>,
    pub sold_at: Option<i64>,
    pub urn: Option<String>,
    pub owner: Option<String>,
    pub image: Option<String>,
    pub issued_id: Option<String>,
    pub item_id: Option<String>,
    pub item_type: Option<String>,
    pub rarity: Option<String>,
    pub category: Option<String>,
    pub name: Option<String>,
    pub body_shapes: Option<Vec<String>>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub wearable_category: Option<String>,
    pub emote_category: Option<String>,
    pub description: Option<String>,
    pub size: Option<i32>,
    pub subdomain: Option<String>,
    #[sqlx(rename = "loop")]
    pub r#loop: Option<bool>,
    pub has_sound: Option<bool>,
    pub has_geometry: Option<bool>,
    pub emote_outcome_type: Option<String>,
    pub estate_parcels: Option<sqlx::types::Json<Vec<EstateParcel>>>,
    pub parcel_estate_token_id: Option<String>,
    pub parcel_estate_name: Option<String>,
    pub parcel_estate_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
pub struct EstateParcel {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Serialize)]
pub struct Nft {
    #[serde(rename = "activeOrderId")]
    pub active_order_id: Option<String>,
    pub category: String,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    #[serde(rename = "contractAddress")]
    pub contract_address: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    pub data: NftData,
    pub id: String,
    pub image: String,
    #[serde(rename = "issuedId")]
    pub issued_id: Option<String>,
    #[serde(rename = "itemId")]
    pub item_id: Option<String>,
    pub name: String,
    pub network: Network,
    #[serde(rename = "openRentalId")]
    pub open_rental_id: Option<String>,
    pub owner: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "soldAt")]
    pub sold_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urn: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum NftData {
    Wearable { wearable: WearableData },
    Emote { emote: EmoteData },
    Parcel { parcel: ParcelData },
    Estate { estate: EstateData },
    Ens { ens: EnsData },
}

#[derive(Debug, Serialize)]
pub struct WearableData {
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub category: String,
    pub description: String,
    pub rarity: String,
    #[serde(rename = "isSmart")]
    pub is_smart: bool,
}

#[derive(Debug, Serialize)]
pub struct EmoteData {
    #[serde(rename = "bodyShapes")]
    pub body_shapes: Vec<String>,
    pub category: String,
    pub description: String,
    pub rarity: String,
    #[serde(rename = "loop")]
    pub r#loop: bool,
    #[serde(rename = "hasSound")]
    pub has_sound: bool,
    #[serde(rename = "hasGeometry")]
    pub has_geometry: bool,
    #[serde(rename = "outcomeType")]
    pub outcome_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParcelData {
    pub x: String,
    pub y: String,
    pub description: Option<String>,
    pub estate: Option<ParcelEstate>,
}

#[derive(Debug, Serialize)]
pub struct ParcelEstate {
    pub name: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
}

#[derive(Debug, Serialize)]
pub struct EstateData {
    pub size: i64,
    pub description: Option<String>,
    pub parcels: Vec<EstateParcel>,
}

#[derive(Debug, Serialize)]
pub struct EnsData {
    pub subdomain: String,
}

#[derive(Debug, Serialize)]
pub struct NftResult {
    pub nft: Nft,
    pub order: Option<serde_json::Value>,
    pub rental: Option<serde_json::Value>,
}

pub struct NftsComponent {
    pool: PgPool,
    rentals: crate::ports::rentals::RentalsComponent,
}

impl NftsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            rentals: crate::ports::rentals::RentalsComponent::new(None),
        }
    }

    pub fn with_rentals(pool: PgPool, rentals: crate::ports::rentals::RentalsComponent) -> Self {
        Self { pool, rentals }
    }

    pub async fn get_nfts(
        &self,
        filters: &NftFilters,
        _caller: Option<String>,
    ) -> Result<(Vec<NftResult>, i64), ApiError> {
        if filters.owner.is_some() && filters.tenant.is_some() {
            return Err(ApiError::bad_request(
                NftErrors::INVALID_SEARCH_BY_TENANT_AND_OWNER,
            ));
        }
        if let Some(ref tid) = filters.token_id {
            if !tid.chars().all(|c| c.is_ascii_digit()) || tid.is_empty() {
                return Err(ApiError::bad_request(NftErrors::INVALID_TOKEN_ID));
            }
            if filters.contract_addresses.is_empty() {
                return Err(ApiError::bad_request(NftErrors::MISSING_CONTRACT_ADDRESS));
            }
        }

        // Port of upstream getNFTFilters: when isOnRent is set for LAND, the
        // rentals listings drive the result set — fetch the OPEN listings first
        // and narrow the NFT query to exactly those nftIds (upstream rewrites
        // filters.ids to the rented nftIds). The fetched listings are also reused
        // below so we don't re-query.
        let wants_rentals = self.rentals.is_enabled()
            && filters.is_on_rent
            && (matches!(
                filters.category,
                Some(NftCategory::Estate) | Some(NftCategory::Parcel)
            ) || filters.is_land);

        let mut effective = filters.clone();
        let mut prefetched_listings: Option<Vec<crate::ports::rentals::RentalListing>> = None;
        if wants_rentals {
            let statuses = if effective.rental_status.is_empty() {
                vec!["open".to_string()]
            } else {
                effective.rental_status.clone()
            };
            // Upstream queries all OPEN rentals (workaround comment in
            // getNFTFilters) and pins filters.ids to their nftIds.
            let listings = self.rentals.get_open_rentals(&statuses).await;
            effective.ids = listings.iter().map(|l| l.nft_id.clone()).collect();
            prefetched_listings = Some(listings);
            // No rented NFTs => empty result without touching the NFT table.
            if effective.ids.is_empty() {
                return Ok((Vec::new(), 0));
            }
        }

        let (sql, binds) = build_nfts_query(&effective);
        let mut q = sqlx::query_as::<_, DbNft>(sqlx::AssertSqlSafe(sql));
        for b in &binds {
            q = match b {
                Bind::Text(s) => q.bind(s.clone()),
                Bind::TextArray(v) => q.bind(v.clone()),
                Bind::Int(i) => q.bind(*i),
                Bind::Float(f) => q.bind(*f),
            };
        }
        let rows: Vec<DbNft> = q.fetch_all(&self.pool).await?;

        let total = rows.first().map(|r| r.count).unwrap_or(0);

        let nft_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let orders_by_nft = if nft_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            self.get_open_orders_by_nft_ids(&nft_ids, effective.owner.as_deref())
                .await?
        };

        // Rental listings for the LAND NFTs on this page. If isOnRent already
        // prefetched the listings, reuse them; otherwise look up the listings for
        // the LAND/Estate ids in the result (mirrors upstream's landNftIds path).
        let listings: Vec<crate::ports::rentals::RentalListing> =
            if let Some(l) = prefetched_listings {
                l
            } else if self.rentals.is_enabled() {
                let land_ids: Vec<String> = rows
                    .iter()
                    .filter(|r| matches!(r.category.as_deref(), Some("parcel") | Some("estate")))
                    .map(|r| r.id.clone())
                    .collect();
                if land_ids.is_empty() {
                    Vec::new()
                } else {
                    let statuses = if filters.rental_status.is_empty() {
                        vec!["open".to_string()]
                    } else {
                        filters.rental_status.clone()
                    };
                    self.rentals
                        .get_rentals_listings_of_nfts(&land_ids, &statuses)
                        .await
                }
            } else {
                Vec::new()
            };
        let listing_by_nft: std::collections::HashMap<&str, &crate::ports::rentals::RentalListing> =
            listings.iter().map(|l| (l.nft_id.as_str(), l)).collect();

        let results = rows
            .iter()
            .map(|r| {
                let order = orders_by_nft.get(&r.id);
                let listing = listing_by_nft.get(r.id.as_str()).copied();
                let mut nft = from_db_nft_to_nft(r);
                nft.active_order_id = order.map(|o| o.id.clone());
                nft.open_rental_id = listing.map(|l| l.id.clone());
                NftResult {
                    nft,
                    order: order
                        .map(|o| serde_json::to_value(o).unwrap_or(serde_json::Value::Null)),
                    rental: listing
                        .map(|l| serde_json::to_value(l).unwrap_or(serde_json::Value::Null)),
                }
            })
            .collect();
        Ok((results, total))
    }

    /// LAND/Estate NFT ids that `owner` currently has an open rental listing for
    /// — upstream's owner-path `rentalAssetsIds` (used to surface assets the
    /// lessor has put up for rent). Exposed for the owner query path.
    pub async fn rental_assets_ids_for_owner(&self, owner: &str) -> Vec<String> {
        self.rentals.get_rental_assets_ids_for_lessor(owner).await
    }

    async fn get_open_orders_by_nft_ids(
        &self,
        nft_ids: &[String],
        owner: Option<&str>,
    ) -> Result<std::collections::HashMap<String, crate::ports::orders::Order>, ApiError> {
        let mut sql = format!(
            r#"
SELECT
  ord.id::text            AS id,
  ''                      AS trade_id,
  ord.marketplace_address AS marketplace_address,
  ord.nft_address         AS nft_address,
  ord.token_id::text      AS token_id,
  ord.price::text         AS price,
  nft.issued_id::text     AS issued_id,
  ord.nft_id              AS nft_id,
  ord.owner               AS owner,
  ord.buyer               AS buyer,
  ord.status              AS status,
  ord.created_at::float8  AS created_at,
  ord.updated_at::float8  AS updated_at,
  ord.expires_at::float8  AS expires_at,
  ord.network             AS network
FROM {schema}."order" ord
JOIN {schema}."nft" nft ON ord.nft_id = nft.id AND nft.owner_address = ord.owner
WHERE ord.status = 'open'
  AND ord.expires_at_normalized > NOW()
  AND ord.nft_id = ANY($1)
"#,
            schema = MARKETPLACE_SQUID_SCHEMA,
        );
        if owner.is_some() {
            sql.push_str(" AND LOWER(ord.owner) = LOWER($2)");
        }

        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(nft_ids);
        if let Some(o) = owner {
            q = q.bind(o.to_string());
        }
        let db_rows = q.fetch_all(&self.pool).await?;

        let mut map = std::collections::HashMap::new();
        for row in &db_rows {
            let nft_id: String = row.try_get("nft_id").unwrap_or_default();
            map.insert(nft_id, crate::ports::orders::row_to_order(row));
        }
        Ok(map)
    }
}

pub enum Bind {
    Text(String),
    TextArray(Vec<String>),
    Int(i64),
    Float(f64),
}

pub fn build_nfts_query(filters: &NftFilters) -> (String, Vec<Bind>) {
    let mut binds: Vec<Bind> = Vec::new();
    let mut next_idx = 1usize;

    fn emit(b: Bind, bs: &mut Vec<Bind>, idx: &mut usize) -> String {
        bs.push(b);
        let s = format!("${}", *idx);
        *idx += 1;
        s
    }

    let mut inner_wheres: Vec<String> = Vec::new();

    if let Some(ref o) = filters.owner {
        let p = emit(Bind::Text(o.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" owner_address = {} ", p));
    }
    if let Some(c) = filters.category {
        let p = emit(
            Bind::Text(nft_category_db_str(c).to_string()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" category = {} ", p));
    }
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" token_id = {}::numeric ", p));
    }
    if let Some(ref iid) = filters.item_id {
        let p = emit(Bind::Text(iid.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" LOWER(item_id) = LOWER({}) ", p));
    }
    if let Some(n) = filters.network {
        let p = emit(
            Bind::TextArray(get_db_networks(n).into_iter().map(String::from).collect()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" network = ANY ({}) ", p));
    }
    if filters.is_wearable_head {
        inner_wheres.push(" search_is_wearable_head = true ".to_string());
    }
    if filters.is_land {
        inner_wheres.push(" search_is_land = true ".to_string());
    }
    if filters.is_wearable_accessory {
        inner_wheres.push(" search_is_wearable_accessory = true ".to_string());
    }
    if filters.is_wearable_smart {
        let p = emit(
            Bind::Text(ItemType::SmartWearableV1.as_str().to_string()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" item_type = {} ", p));
    }
    if !filters.contract_addresses.is_empty() {
        let p = emit(
            Bind::TextArray(filters.contract_addresses.clone()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" contract_address = ANY ({}) ", p));
    }
    if let Some(ref s) = filters.search {
        let p = emit(Bind::Text(s.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_text % {} ", p));
    }
    if let Some(mn) = filters.min_distance_to_plaza {
        let p = emit(Bind::Float(mn), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_distance_to_plaza >= {} ", p));
    }
    if let Some(mx) = filters.max_distance_to_plaza {
        let p = emit(Bind::Float(mx), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_distance_to_plaza <= {} ", p));
    }
    if filters.adjacent_to_road {
        inner_wheres.push(" search_adjacent_to_road = true ".to_string());
    }
    if !filters.ids.is_empty() {
        let p = emit(
            Bind::TextArray(filters.ids.clone()),
            &mut binds,
            &mut next_idx,
        );
        inner_wheres.push(format!(" id = ANY ({}) ", p));
    }
    if let Some(ref mn) = filters.min_price {
        let p = emit(Bind::Text(mn.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_order_price >= {}::numeric ", p));
    }
    if let Some(ref mx) = filters.max_price {
        let p = emit(Bind::Text(mx.clone()), &mut binds, &mut next_idx);
        inner_wheres.push(format!(" search_order_price <= {}::numeric ", p));
    }
    if filters.is_land
        || filters.category == Some(NftCategory::Parcel)
        || filters.category == Some(NftCategory::Estate)
    {
        inner_wheres.push(" search_estate_size > 0 ".to_string());
    }

    let inner_where = where_from(&inner_wheres);

    let inner_sort = match filters.sort_by {
        Some(NftSortBy::Name) => " ORDER BY name ASC, id ASC ",
        Some(NftSortBy::Newest) => " ORDER BY created_at DESC, id ASC ",
        Some(NftSortBy::RecentlySold) => " ORDER BY sold_at DESC, id ASC ",
        // cheapest_parcel == upstream NFTSortBy.CHEAPEST (land price ASC).
        // search_order_price is NUMERIC, so this is a numeric (not lexical)
        // sort; NULLS LAST keeps unlisted parcels after listed ones.
        Some(NftSortBy::CheapestParcel) => " ORDER BY search_order_price ASC NULLS LAST, id ASC ",
        _ => "",
    };
    let apply_inner_limit =
        !matches!(filters.sort_by, Some(NftSortBy::RecentlyListed)) && filters.owner.is_none();
    let limit_val = clamp_first(filters.first, 100);
    let offset_val = clamp_skip(filters.skip);
    let inner_limit_offset = if apply_inner_limit {
        let lp = emit(Bind::Int(limit_val), &mut binds, &mut next_idx);
        let op = emit(Bind::Int(offset_val), &mut binds, &mut next_idx);
        format!(" LIMIT {} OFFSET {} ", lp, op)
    } else {
        String::new()
    };

    let mut estate_wheres: Vec<String> = Vec::new();
    if let Some(mn) = filters.min_estate_size {
        let p = emit(Bind::Float(mn), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.size >= {} ", p));
    } else {
        estate_wheres.push(" est.size > 0 ".to_string());
    }
    if let Some(mx) = filters.max_estate_size {
        let p = emit(Bind::Float(mx), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.size <= {} ", p));
    }
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        estate_wheres.push(format!(" est.token_id = {}::numeric ", p));
    }
    let estate_where = where_from(&estate_wheres);

    let mut parcel_wheres: Vec<String> = Vec::new();
    if let Some(ref tid) = filters.token_id {
        let p = emit(Bind::Text(tid.clone()), &mut binds, &mut next_idx);
        parcel_wheres.push(format!(
            " (par.token_id = {p}::numeric OR par_est.token_id = {p}::numeric) ",
            p = p
        ));
    }
    let parcel_where = where_from(&parcel_wheres);

    let trades_cat = if let Some(c) = filters.category {
        let p = emit(
            Bind::Text(nft_category_db_str(c).to_string()),
            &mut binds,
            &mut next_idx,
        );
        format!(" WHERE sent_nft_category = {} ", p)
    } else {
        String::new()
    };

    let mut outer_wheres: Vec<String> = Vec::new();
    if filters.emote_has_sound {
        outer_wheres.push(" emote.has_sound = true ".to_string());
    }
    if filters.emote_has_geometry {
        outer_wheres.push(" emote.has_geometry = true ".to_string());
    }
    if filters.emote_outcome_type.is_some() {
        outer_wheres.push(" emote.outcome_type IS NOT NULL ".to_string());
    }
    if let Some(ref ec) = filters.emote_category {
        let p = emit(Bind::Text(ec.clone()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" emote.category = {} ", p));
    }
    if let Some(ref wc) = filters.wearable_category {
        let p = emit(Bind::Text(wc.clone()), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" wearable.category = {} ", p));
    }
    if let Some(mode) = emote_play_mode_clause(&filters.emote_play_mode) {
        outer_wheres.push(format!(" nft.search_emote_loop = {} ", mode));
    }
    if let Some(arr) = body_shapes_for_genders(&filters.emote_genders) {
        let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" nft.search_emote_body_shapes @> {} ", p));
    }
    if let Some(arr) = body_shapes_for_genders(&filters.wearable_genders) {
        let p = emit(Bind::TextArray(arr), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" nft.search_wearable_body_shapes @> {} ", p));
    }
    if !filters.creator.is_empty() {
        let lower: Vec<String> = filters.creator.iter().map(|c| c.to_lowercase()).collect();
        let p = emit(Bind::TextArray(lower), &mut binds, &mut next_idx);
        outer_wheres.push(format!(" LOWER(item.creator) = ANY({}) ", p));
    }
    if !filters.item_rarities.is_empty() {
        let p = emit(
            Bind::TextArray(filters.item_rarities.clone()),
            &mut binds,
            &mut next_idx,
        );
        outer_wheres.push(format!(
            " (nft.search_wearable_rarity = ANY ({p}) OR nft.search_emote_rarity = ANY ({p})) ",
            p = p
        ));
    }
    match filters.is_on_sale {
        Some(true) => {
            outer_wheres.push(format!(
                " (trades.id IS NOT NULL OR (nft.search_order_status = 'open' \
                  AND nft.search_order_expires_at < {max_ts} \
                  AND ((LENGTH(nft.search_order_expires_at::text) = 13 \
                        AND TO_TIMESTAMP(nft.search_order_expires_at / 1000.0) > NOW()) \
                     OR (LENGTH(nft.search_order_expires_at::text) = 10 \
                        AND TO_TIMESTAMP(nft.search_order_expires_at) > NOW())))) ",
                max_ts = MAX_ORDER_TIMESTAMP
            ));
        }
        Some(false) => {
            outer_wheres
                .push(" (trades.id IS NULL AND nft.search_order_status IS NULL) ".to_string());
        }
        None => {}
    }
    if !filters.banned_names.is_empty() {
        let p = emit(
            Bind::TextArray(filters.banned_names.clone()),
            &mut binds,
            &mut next_idx,
        );
        outer_wheres.push(format!(
            " (nft.category != 'ens' OR nft.name <> ALL ({})) ",
            p
        ));
    }

    let outer_where = where_from(&outer_wheres);

    // Every ORDER BY carries `, nft.id ASC` as a stable tie-breaker so
    // LIMIT/OFFSET pages partition deterministically (no row appearing on two
    // pages or being skipped when the primary key ties).
    let main_sort = match filters.sort_by {
        Some(NftSortBy::RecentlyListed) => " ORDER BY order_created_at DESC, nft.id ASC ",
        Some(NftSortBy::Name) => " ORDER BY name ASC, nft.id ASC ",
        Some(NftSortBy::Newest) => " ORDER BY created_at DESC, nft.id ASC ",
        Some(NftSortBy::RecentlySold) => " ORDER BY sold_at DESC, nft.id ASC ",
        Some(NftSortBy::CheapestParcel) => " ORDER BY order_price ASC NULLS LAST, nft.id ASC ",
        _ => "",
    };

    let outer_limit_offset = if apply_inner_limit {
        String::new()
    } else {
        let lp = emit(Bind::Int(limit_val), &mut binds, &mut next_idx);
        let op = emit(Bind::Int(offset_val), &mut binds, &mut next_idx);
        format!(" LIMIT {} OFFSET {} ", lp, op)
    };

    // Off-chain trades (Marketplace v3). Port of upstream `getTradesCTE`
    // (catalog/queries.ts): the NFT query joins the real off-chain-order
    // materialized view so that NFTs listed only through a v3 public_nft_order
    // surface activeOrderId / order / price / isOnSale, and `order_created_at`
    // (the recently_listed sort key) coalesces in the trade's created_at.
    // `trades_cat` is the optional `WHERE sent_nft_category = $N` narrowing.
    let sql = format!(
        "WITH unified_trades AS (
            SELECT * FROM marketplace.mv_trades {trades_cat}
         ),
         filtered_estate AS (
            SELECT est.id, est.token_id, est.size, est.data_id,
                -- JSONB_AGG (one jsonb array), not ARRAY_AGG(JSON_BUILD_OBJECT)
                -- which yields json[] and mismatches the Json<Vec<_>> decode.
                JSONB_AGG(JSONB_BUILD_OBJECT('x', est_parcel.x, 'y', est_parcel.y)) AS estate_parcels
            FROM {schema}.estate est
            LEFT JOIN {schema}.parcel est_parcel ON est.id = est_parcel.estate_id
            {estate_where}
            GROUP BY est.id, est.token_id, est.size, est.data_id
         ),
         parcel_estate_data AS (
            SELECT par.*, par_est.token_id::text AS parcel_estate_token_id,
                   est_data.name AS parcel_estate_name
            FROM {schema}.parcel par
            LEFT JOIN {schema}.estate par_est ON par.estate_id = par_est.id AND par_est.size > 0
            LEFT JOIN {schema}.data est_data ON par_est.data_id = est_data.id
            {parcel_where}
         ),
         filtered_nft AS (
            SELECT * FROM {schema}.nft {inner_where} {inner_sort} {inner_limit_offset}
         )
         SELECT
            COUNT(*) OVER() AS count,
            nft.id,
            nft.contract_address,
            nft.token_id::text as token_id,
            nft.network,
            nft.created_at::int8 as created_at,
            nft.token_uri AS url,
            nft.updated_at::int8 as updated_at,
            nft.sold_at::int8 as sold_at,
            nft.urn,
            account.address AS owner,
            nft.image,
            nft.issued_id::text AS issued_id,
            item.blockchain_id::text AS item_id,
            nft.category,
            COALESCE(wearable.rarity, emote.rarity) AS rarity,
            COALESCE(wearable.name, emote.name, land_data.name, ens.subdomain) AS name,
            parcel.x::text AS x,
            parcel.y::text AS y,
            ens.subdomain,
            wearable.body_shapes,
            wearable.category AS wearable_category,
            emote.category AS emote_category,
            nft.item_type,
            emote.loop,
            emote.has_sound,
            emote.has_geometry,
            emote.outcome_type AS emote_outcome_type,
            estate.estate_parcels,
            estate.size::int4 AS size,
            parcel.parcel_estate_token_id,
            parcel.parcel_estate_name,
            parcel.estate_id AS parcel_estate_id,
            COALESCE(wearable.description, emote.description, land_data.description) AS description,
            -- Sort key for sortBy=recently_listed. search_order_created_at is a
            -- NUMERIC unix-epoch on the nft row (NULL when not listed); upstream
            -- coalesces it with the trade created_at. The unified_trades CTE is
            -- the empty stub here, so trades.created_at is always NULL -- kept for
            -- parity. Without this projection the ORDER BY references a column that
            -- does not exist (column order_created_at does not exist) -> 500.
            COALESCE(TO_TIMESTAMP(nft.search_order_created_at), trades.created_at) AS order_created_at,
            -- Numeric listing price, projected so the outer ORDER BY for
            -- sortBy=cheapest_parcel sorts numerically (NULL when unlisted).
            nft.search_order_price AS order_price
         FROM filtered_nft nft
         LEFT JOIN {schema}.metadata metadata ON nft.metadata_id = metadata.id
         LEFT JOIN {schema}.wearable wearable ON metadata.wearable_id = wearable.id
         LEFT JOIN {schema}.emote emote ON metadata.emote_id = emote.id
         LEFT JOIN parcel_estate_data parcel ON nft.id = parcel.id
         LEFT JOIN filtered_estate estate ON nft.id = estate.id
         LEFT JOIN {schema}.data land_data ON (estate.data_id = land_data.id OR parcel.id = land_data.id)
         LEFT JOIN {schema}.ens ens ON ens.id = nft.ens_id
         LEFT JOIN {schema}.account account ON nft.owner_id = account.id
         LEFT JOIN {schema}.item item ON item.id = nft.item_id
         LEFT JOIN unified_trades trades ON trades.sent_contract_address = nft.contract_address
            AND trades.sent_token_id::numeric = nft.token_id
            AND trades.status = 'open' AND trades.signer = account.address
         {outer_where}
         {main_sort}
         {outer_limit_offset}",
        schema = MARKETPLACE_SQUID_SCHEMA,
        trades_cat = trades_cat,
        estate_where = estate_where,
        parcel_where = parcel_where,
        inner_where = inner_where,
        inner_sort = inner_sort,
        inner_limit_offset = inner_limit_offset,
        outer_where = outer_where,
        main_sort = main_sort,
        outer_limit_offset = outer_limit_offset,
    );

    (sql, binds)
}

fn nft_category_db_str(c: NftCategory) -> &'static str {
    match c {
        NftCategory::Parcel => "parcel",
        NftCategory::Estate => "estate",
        NftCategory::Wearable => "wearable",
        NftCategory::Ens => "ens",
        NftCategory::Emote => "emote",
    }
}

fn body_shapes_for_genders(genders: &[String]) -> Option<Vec<String>> {
    if genders.is_empty() {
        return None;
    }
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

fn emote_play_mode_clause(modes: &[String]) -> Option<bool> {
    if modes.is_empty() || modes.len() == 2 {
        return None;
    }
    if modes.iter().any(|m| m == "loop") {
        Some(true)
    } else {
        Some(false)
    }
}

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
        image: fix_urn(&d.image.clone().unwrap_or_default()),
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
