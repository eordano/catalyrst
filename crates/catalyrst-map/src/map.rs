use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;
use sqlx::PgPool;

use crate::rentals::{RentalsClient, TileRentalListing};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TileType {
    Owned,
    Unowned,
    Plaza,
    Road,
    District,
}

impl TileType {
    fn from_str(s: &str) -> Option<TileType> {
        match s {
            "owned" => Some(TileType::Owned),
            "unowned" => Some(TileType::Unowned),
            "plaza" => Some(TileType::Plaza),
            "road" => Some(TileType::Road),
            "district" => Some(TileType::District),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Tile {
    pub id: String,
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "Option::is_none", rename = "nftId")]
    pub nft_id: Option<String>,
    #[serde(rename = "type")]
    pub tile_type: TileType,
    pub top: bool,
    pub left: bool,
    #[serde(rename = "topLeft")]
    pub top_left: bool,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "estateId")]
    pub estate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tokenId")]
    pub token_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "expiresAt")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "rentalListing")]
    pub rental_listing: Option<TileRentalListing>,
    // --- Additive estate/rental detail enrichment (beyond upstream `Tile`) ---
    // These are emitted only when present, so strict upstream-parity consumers
    // (which ignore unknown keys) are unaffected and existing fields keep their
    // shape/order. They surface estate size + the rental's headline price/expiry
    // so callers don't have to re-derive them from `rentalListing.periods`.
    /// Number of parcels in the estate this tile belongs to (0/absent for loose parcels).
    #[serde(skip_serializing_if = "Option::is_none", rename = "estateSize")]
    pub estate_size: Option<i32>,
    /// Max price-per-day (wei, as string) across the open rental listing's periods.
    #[serde(skip_serializing_if = "Option::is_none", rename = "rentalPricePerDay")]
    pub rental_price_per_day: Option<String>,
    /// UTC ms timestamp of the rental listing signature expiration.
    #[serde(skip_serializing_if = "Option::is_none", rename = "rentalExpiresAt")]
    pub rental_expires_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SpecialTile {
    #[serde(rename = "type")]
    tile_type: String,
    top: bool,
    left: bool,
    #[serde(rename = "topLeft")]
    top_left: bool,
    id: String,
    #[serde(default)]
    name: Option<String>,
}

const SPECIAL_TILES_JSON: &str = include_str!("../data/specialTiles.json");

pub struct MapData {
    pub tiles: HashMap<String, Tile>,
    pub last_updated_at: i64,
}

/// A generation-keyed byte cache. Entries are valid only while `keyed_at`
/// equals the current dataset `last_updated_at`; the first access after a
/// refresh observes a generation mismatch and clears the cache, so stored
/// bytes can never outlive the snapshot they were derived from.
#[derive(Default)]
struct GenCache {
    keyed_at: i64,
    entries: HashMap<String, Arc<Vec<u8>>>,
}

#[derive(Clone)]
pub struct MapComponent {
    pool: PgPool,
    schema: String,
    land_contract: String,
    estate_contract: String,
    special_tiles: Arc<HashMap<String, SpecialTile>>,
    rentals: Option<RentalsClient>,
    data: Arc<RwLock<Option<Arc<MapData>>>>,
    tiles_cache: Arc<RwLock<GenCache>>,
    png_cache: Arc<RwLock<GenCache>>,
}

#[inline]
pub fn coords_to_id(x: i32, y: i32) -> String {
    format!("{},{}", x, y)
}

impl MapComponent {
    pub fn new(
        pool: PgPool,
        schema: String,
        land_contract: String,
        estate_contract: String,
    ) -> Self {
        let special: HashMap<String, SpecialTile> =
            serde_json::from_str(SPECIAL_TILES_JSON).expect("specialTiles.json must parse");
        Self {
            pool,
            schema,
            land_contract,
            estate_contract,
            special_tiles: Arc::new(special),
            rentals: RentalsClient::from_env(),
            data: Arc::new(RwLock::new(None)),
            tiles_cache: Arc::new(RwLock::new(GenCache::default())),
            png_cache: Arc::new(RwLock::new(GenCache::default())),
        }
    }

    pub fn rentals_enabled(&self) -> bool {
        self.rentals.is_some()
    }

    pub fn cached_tiles_response(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let last = self.last_updated_at();
        let cache = self.tiles_cache.read();
        if cache.keyed_at == last {
            cache.entries.get(key).cloned()
        } else {
            None
        }
    }

    pub fn store_tiles_response(&self, key: String, body: Arc<Vec<u8>>) {
        let last = self.last_updated_at();
        let mut cache = self.tiles_cache.write();
        if cache.keyed_at != last {
            cache.keyed_at = last;
            cache.entries.clear();
        }
        cache.entries.insert(key, body);
    }

    /// Look up rendered PNG bytes for `key`, valid only for the current
    /// dataset generation. A miss after a refresh is expected (the generation
    /// no longer matches), forcing a re-render against the fresh snapshot.
    pub fn cached_png(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let last = self.last_updated_at();
        let cache = self.png_cache.read();
        if cache.keyed_at == last {
            cache.entries.get(key).cloned()
        } else {
            None
        }
    }

    pub fn store_png(&self, key: String, body: Arc<Vec<u8>>) {
        let last = self.last_updated_at();
        let mut cache = self.png_cache.write();
        if cache.keyed_at != last {
            cache.keyed_at = last;
            cache.entries.clear();
        }
        cache.entries.insert(key, body);
    }

    pub fn is_ready(&self) -> bool {
        self.data.read().is_some()
    }

    pub fn snapshot(&self) -> Option<Arc<MapData>> {
        self.data.read().clone()
    }

    pub fn last_updated_at(&self) -> i64 {
        self.data
            .read()
            .as_ref()
            .map(|d| d.last_updated_at)
            .unwrap_or(0)
    }

    pub fn land_contract(&self) -> &str {
        &self.land_contract
    }

    pub fn estate_contract(&self) -> &str {
        &self.estate_contract
    }

    pub async fn refresh(&self) -> anyhow::Result<()> {
        let data = self.build().await?;
        *self.data.write() = Some(Arc::new(data));
        Ok(())
    }

    async fn build(&self) -> anyhow::Result<MapData> {
        let sql = format!(
            r#"
            SELECT
                p.search_parcel_x::int4              AS x,
                p.search_parcel_y::int4              AS y,
                p.id                                 AS nft_id,
                p.token_id::text                     AS token_id,
                p.name                               AS parcel_name,
                p.owner_address                      AS parcel_owner,
                p.updated_at::int8                   AS parcel_updated_at,
                p.search_parcel_estate_id            AS estate_full_id,
                p.search_order_price::text           AS parcel_order_price,
                p.search_order_expires_at::int8      AS parcel_order_expires_at,
                e.name                               AS estate_name,
                e.owner_address                      AS estate_owner,
                e.updated_at::int8                   AS estate_updated_at,
                e.search_order_price::text           AS estate_order_price,
                e.search_order_expires_at::int8      AS estate_order_expires_at,
                e.search_estate_size::int4           AS estate_size
            FROM {schema}.nft p
            LEFT JOIN {schema}.nft e
                   ON e.id = p.search_parcel_estate_id
                  AND e.category = 'estate'
            WHERE p.category = 'parcel'
              AND p.search_parcel_x IS NOT NULL
              AND p.search_parcel_y IS NOT NULL
            "#,
            schema = self.schema
        );

        let rows = sqlx::query_as::<_, ParcelRow>(&sql)
            .fetch_all(&self.pool)
            .await?;

        let rental_listings: HashMap<String, TileRentalListing> = match &self.rentals {
            Some(client) => match client.fetch_open_listings().await {
                Ok(listings) => listings,
                Err(e) => {
                    tracing::warn!(error = %e, "rental listings fetch failed; serving tiles without rentalListing");
                    HashMap::new()
                }
            },
            None => HashMap::new(),
        };

        let now_ms = chrono::Utc::now().timestamp_millis();
        let now_secs = now_ms / 1000;

        let mut tiles: HashMap<String, Tile> =
            HashMap::with_capacity(rows.len() + self.special_tiles.len());
        let mut last_updated_at: i64 = 0;

        for r in &rows {
            let id = coords_to_id(r.x, r.y);

            let name = r.estate_name.clone().or_else(|| r.parcel_name.clone());
            let owner = r.estate_owner.clone().or_else(|| r.parcel_owner.clone());

            let rental_key = match &r.estate_full_id {
                Some(full) if !full.is_empty() => full.as_str(),
                _ => r.nft_id.as_str(),
            };
            let rental_listing = rental_listings.get(rental_key).cloned();

            let updated_at = (r.estate_updated_at.unwrap_or(0) * 1000)
                .max(r.parcel_updated_at * 1000)
                .max(rental_listing.as_ref().map(|rl| rl.updated_at).unwrap_or(0));
            last_updated_at = last_updated_at.max(updated_at);

            let special = self.special_tiles.get(&id);

            let tile_type = if let Some(s) = special {
                TileType::from_str(&s.tile_type).unwrap_or(TileType::Unowned)
            } else if owner.is_some() {
                TileType::Owned
            } else {
                TileType::Unowned
            };

            let mut tile = Tile {
                id: id.clone(),
                x: r.x,
                y: r.y,
                nft_id: Some(r.nft_id.clone()),
                tile_type,
                top: special.map(|s| s.top).unwrap_or(false),
                left: special.map(|s| s.left).unwrap_or(false),
                top_left: special.map(|s| s.top_left).unwrap_or(false),
                updated_at,
                name,
                owner,
                estate_id: None,
                token_id: Some(r.token_id.clone()),
                price: None,
                expires_at: None,
                estate_size: r.estate_size.filter(|s| *s > 0),
                rental_price_per_day: rental_listing.as_ref().map(|rl| rl.max_price_per_day()),
                rental_expires_at: rental_listing.as_ref().map(|rl| rl.expiration),
                rental_listing,
            };

            if let Some(full) = &r.estate_full_id {
                if !full.is_empty() {
                    tile.estate_id = full.rsplit('-').next().map(|s| s.to_string());
                }
            }

            let (price_str, expires) = if r
                .estate_full_id
                .as_deref()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
                && r.estate_order_price.is_some()
            {
                (r.estate_order_price.clone(), r.estate_order_expires_at)
            } else {
                (r.parcel_order_price.clone(), r.parcel_order_expires_at)
            };

            if let (Some(price_str), Some(expires_at_ms)) = (price_str, expires) {
                let expires_secs = if expires_at_ms.to_string().len() == 10 {
                    expires_at_ms
                } else {
                    (expires_at_ms as f64 / 1000.0).round() as i64
                };
                if expires_secs > now_secs {
                    if let Ok(wei) = price_str.parse::<f64>() {
                        tile.price = Some((wei / 1e18).round());
                        tile.expires_at = Some(expires_secs);
                    }
                }
            }

            tiles.insert(id, tile);
        }

        let mut ids: Vec<(i32, i32)> = tiles.values().map(|t| (t.x, t.y)).collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        for (x, y) in ids {
            compute_estate(x, y, &mut tiles);
        }

        for st in self.special_tiles.values() {
            if tiles.contains_key(&st.id) {
                continue;
            }
            let coords: Vec<&str> = st.id.split(',').collect();
            if coords.len() != 2 {
                continue;
            }
            let (Ok(x), Ok(y)) = (coords[0].parse::<i32>(), coords[1].parse::<i32>()) else {
                continue;
            };
            let tile_type = TileType::from_str(&st.tile_type).unwrap_or(TileType::Unowned);
            let name = st.name.clone().unwrap_or_else(|| {
                let mut c = st.tile_type.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => st.tile_type.clone(),
                }
            });
            tiles.insert(
                st.id.clone(),
                Tile {
                    id: st.id.clone(),
                    x,
                    y,
                    nft_id: None,
                    tile_type,
                    top: st.top,
                    left: st.left,
                    top_left: st.top_left,
                    updated_at: now_ms,
                    name: Some(name),
                    owner: None,
                    estate_id: None,
                    token_id: None,
                    price: None,
                    expires_at: None,
                    estate_size: None,
                    rental_price_per_day: None,
                    rental_expires_at: None,
                    rental_listing: None,
                },
            );
        }

        Ok(MapData {
            tiles,
            last_updated_at,
        })
    }
}

fn compute_estate(x: i32, y: i32, tiles: &mut HashMap<String, Tile>) {
    let id = coords_to_id(x, y);
    let (is_owned_estate, estate_id) = match tiles.get(&id) {
        Some(t) if t.tile_type == TileType::Owned && t.estate_id.is_some() => {
            (true, t.estate_id.clone())
        }
        _ => (false, None),
    };
    if !is_owned_estate {
        return;
    }
    let estate_id = estate_id.unwrap();

    let top = tiles
        .get(&coords_to_id(x, y + 1))
        .map(|t| t.estate_id.as_deref() == Some(estate_id.as_str()))
        .unwrap_or(false);
    let left = tiles
        .get(&coords_to_id(x - 1, y))
        .map(|t| t.estate_id.as_deref() == Some(estate_id.as_str()))
        .unwrap_or(false);
    let top_left = tiles
        .get(&coords_to_id(x - 1, y + 1))
        .map(|t| t.estate_id.as_deref() == Some(estate_id.as_str()))
        .unwrap_or(false);

    if let Some(t) = tiles.get_mut(&id) {
        t.top = top;
        t.left = left;
        t.top_left = top_left;
    }
}

#[derive(sqlx::FromRow)]
struct ParcelRow {
    x: i32,
    y: i32,
    nft_id: String,
    token_id: String,
    parcel_name: Option<String>,
    parcel_owner: Option<String>,
    parcel_updated_at: i64,
    estate_full_id: Option<String>,
    parcel_order_price: Option<String>,
    parcel_order_expires_at: Option<i64>,
    estate_name: Option<String>,
    estate_owner: Option<String>,
    estate_updated_at: Option<i64>,
    estate_order_price: Option<String>,
    estate_order_expires_at: Option<i64>,
    estate_size: Option<i32>,
}
