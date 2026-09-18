use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::rentals::{RentalsClient, TileRentalListing};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "map/", rename_all = "lowercase")
)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "map/"))]
pub struct Tile {
    pub id: String,
    pub x: i32,
    pub y: i32,
    #[serde(rename = "nftId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub nft_id: Option<String>,
    #[serde(rename = "type")]
    pub tile_type: TileType,
    pub top: bool,
    pub left: bool,
    #[serde(rename = "topLeft")]
    pub top_left: bool,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub owner: Option<String>,
    #[serde(rename = "estateId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub estate_id: Option<String>,
    #[serde(rename = "tokenId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub token_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub price: Option<f64>,
    #[serde(rename = "expiresAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub expires_at: Option<i64>,
    #[serde(rename = "rentalListing")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub rental_listing: Option<TileRentalListing>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "map/"))]
pub struct LegacyTile {
    #[serde(rename = "type")]
    pub tile_type: i32,
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub top: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub left: Option<u8>,
    #[serde(rename = "topLeft")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub top_left: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub estate_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub price: Option<f64>,
    #[serde(rename = "rentalPricePerDay")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub rental_price_per_day: Option<String>,
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
    /// estate_id -> coords of its OWNED parcels (drives estate map.png selection).
    pub estates_owned: HashMap<String, Vec<(i32, i32)>>,
    /// estate_id -> coords of ALL parcels carrying it (fallback selection).
    pub estates_all: HashMap<String, Vec<(i32, i32)>>,
}

/// Ceiling on how long a grid is served without a rebuild, whatever the fingerprint says.
pub const DEFAULT_FORCE_REBUILD_AFTER: Duration = Duration::from_secs(15 * 60);

/// Cheap summary of everything the grid build reads. Equal fingerprints mean the
/// build would produce the same tiles, so the 92k-row rebuild can be skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridFingerprint {
    /// count(*) over parcels and estates.
    pub rows: i64,
    /// max(updated_at) over parcels and estates. The squid bumps it on every path
    /// that touches a grid column: transfers (which also carry estate membership
    /// changes), order create/cancel/execute, and parcel or estate metadata updates.
    pub max_updated_at: i64,
    /// Hash of the open rental listings; 0 when rentals are off.
    pub rentals: u64,
}

/// What the current grid was built from, kept to decide whether the next refresh may skip.
#[derive(Debug, Clone)]
pub struct BuildStamp {
    pub fingerprint: Option<GridFingerprint>,
    pub built_at: Instant,
    /// Earliest expiry (unix seconds) among the orders the grid prices. Once it passes a
    /// tile must drop its price even though nothing in the database moved.
    pub next_order_expiry: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildReason {
    Initial,
    FingerprintUnavailable,
    FingerprintChanged,
    OrderExpired,
    ForceInterval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Rebuilt(RebuildReason),
    Skipped,
}

/// Decides whether a refresh must rebuild the grid. `current` is the fingerprint read
/// just before this refresh (None when the query failed), `now_secs` is wall-clock
/// unix time for the order-expiry check.
pub fn rebuild_reason(
    prev: Option<&BuildStamp>,
    current: Option<&GridFingerprint>,
    now: Instant,
    now_secs: i64,
    force_after: Duration,
) -> Option<RebuildReason> {
    let Some(prev) = prev else {
        return Some(RebuildReason::Initial);
    };
    let Some(current) = current else {
        return Some(RebuildReason::FingerprintUnavailable);
    };
    if prev.fingerprint.as_ref() != Some(current) {
        return Some(RebuildReason::FingerprintChanged);
    }
    if prev.next_order_expiry.is_some_and(|t| t <= now_secs) {
        return Some(RebuildReason::OrderExpired);
    }
    if now.saturating_duration_since(prev.built_at) >= force_after {
        return Some(RebuildReason::ForceInterval);
    }
    None
}

/// The change-detection query. Its predicate is exactly the one on the partial index
/// `cat_nft_land_updated_at_idx` (the deployment squid index SQL), so the max is
/// a one-entry probe and the count an index-only scan.
pub fn fingerprint_sql(schema: &str) -> String {
    format!(
        "SELECT count(*)::int8 AS rows, coalesce(max(updated_at), 0)::int8 AS max_updated_at \
         FROM {schema}.nft WHERE category IN ('parcel', 'estate')"
    )
}

/// Order-independent hash of the open rental listings, 0 for none.
pub fn rentals_hash(listings: &HashMap<String, TileRentalListing>) -> u64 {
    if listings.is_empty() {
        return 0;
    }
    let mut keys: Vec<&String> = listings.keys().collect();
    keys.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for k in keys {
        let l = &listings[k];
        k.hash(&mut h);
        l.expiration.hash(&mut h);
        l.updated_at.hash(&mut h);
        for p in &l.periods {
            p.min_days.hash(&mut h);
            p.max_days.hash(&mut h);
            p.price_per_day.hash(&mut h);
        }
    }
    h.finish()
}

/// Price in MANA and expiry in unix seconds for an order that is still live at `now_secs`.
/// `expires_at` arrives in seconds (10 digits) or milliseconds.
pub fn live_order_price(
    price_wei: Option<&str>,
    expires_at: Option<i64>,
    now_secs: i64,
) -> Option<(f64, i64)> {
    let (price_wei, expires_at) = (price_wei?, expires_at?);
    let expires_secs = if expires_at.to_string().len() == 10 {
        expires_at
    } else {
        (expires_at as f64 / 1000.0).round() as i64
    };
    if expires_secs <= now_secs {
        return None;
    }
    let wei = price_wei.parse::<f64>().ok()?;
    Some(((wei / 1e18).round(), expires_secs))
}

/// Wholesale-cleared whenever the map build yields a new `keyed_at`; the LRU cap bounds
/// growth between builds.
struct GenCache {
    keyed_at: i64,
    entries: lru::LruCache<String, Arc<Vec<u8>>>,
}

impl GenCache {
    fn new(cap: NonZeroUsize) -> Self {
        Self {
            keyed_at: 0,
            entries: lru::LruCache::new(cap),
        }
    }
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
    stamp: Arc<RwLock<Option<BuildStamp>>>,
    force_rebuild_after: Duration,
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
        tiles_cache_entries: usize,
        png_cache_entries: usize,
    ) -> Self {
        let special: HashMap<String, SpecialTile> =
            serde_json::from_str(SPECIAL_TILES_JSON).expect("specialTiles.json must parse");
        let tiles_cap = NonZeroUsize::new(tiles_cache_entries.max(1)).unwrap();
        let png_cap = NonZeroUsize::new(png_cache_entries.max(1)).unwrap();
        Self {
            pool,
            schema,
            land_contract,
            estate_contract,
            special_tiles: Arc::new(special),
            rentals: RentalsClient::from_env(),
            data: Arc::new(RwLock::new(None)),
            stamp: Arc::new(RwLock::new(None)),
            force_rebuild_after: DEFAULT_FORCE_REBUILD_AFTER,
            tiles_cache: Arc::new(RwLock::new(GenCache::new(tiles_cap))),
            png_cache: Arc::new(RwLock::new(GenCache::new(png_cap))),
        }
    }

    pub fn with_force_rebuild_after(mut self, force_after: Duration) -> Self {
        self.force_rebuild_after = force_after;
        self
    }

    pub fn build_stamp(&self) -> Option<BuildStamp> {
        self.stamp.read().clone()
    }

    #[cfg(test)]
    pub fn tiles_cache_len(&self) -> usize {
        self.tiles_cache.read().entries.len()
    }

    pub fn cached_tiles_response(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let last = self.last_updated_at();
        let cache = self.tiles_cache.read();
        if cache.keyed_at == last {
            cache.entries.peek(key).cloned()
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
        cache.entries.put(key, body);
    }

    pub fn cached_png(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let last = self.last_updated_at();
        let cache = self.png_cache.read();
        if cache.keyed_at == last {
            cache.entries.peek(key).cloned()
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
        cache.entries.put(key, body);
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

    /// Rebuilds the grid only when its inputs moved since the last build (or the last
    /// build is older than `force_rebuild_after`, or a priced order expired). The
    /// fingerprint is read before the build so a change landing in between can only
    /// cause one extra rebuild, never a stale grid.
    pub async fn refresh(&self) -> anyhow::Result<RefreshOutcome> {
        let rental_listings = self.fetch_rental_listings().await;
        let fingerprint = match self.fingerprint(&rental_listings).await {
            Ok(fp) => Some(fp),
            Err(e) => {
                tracing::warn!(error = %e, "grid fingerprint failed; rebuilding unconditionally");
                None
            }
        };
        let reason = rebuild_reason(
            self.stamp.read().as_ref(),
            fingerprint.as_ref(),
            Instant::now(),
            chrono::Utc::now().timestamp(),
            self.force_rebuild_after,
        );
        let Some(reason) = reason else {
            return Ok(RefreshOutcome::Skipped);
        };
        let (data, next_order_expiry) = self.build(rental_listings).await?;
        *self.data.write() = Some(Arc::new(data));
        *self.stamp.write() = Some(BuildStamp {
            fingerprint,
            built_at: Instant::now(),
            next_order_expiry,
        });
        Ok(RefreshOutcome::Rebuilt(reason))
    }

    async fn fingerprint(
        &self,
        rental_listings: &HashMap<String, TileRentalListing>,
    ) -> anyhow::Result<GridFingerprint> {
        let (rows, max_updated_at): (i64, i64) =
            sqlx::query_as(sqlx::AssertSqlSafe(fingerprint_sql(&self.schema)))
                .fetch_one(&self.pool)
                .await?;
        Ok(GridFingerprint {
            rows,
            max_updated_at,
            rentals: rentals_hash(rental_listings),
        })
    }

    async fn fetch_rental_listings(&self) -> HashMap<String, TileRentalListing> {
        match &self.rentals {
            Some(client) => match client.fetch_open_listings().await {
                Ok(listings) => listings,
                Err(e) => {
                    tracing::warn!(error = %e, "rental listings fetch failed; serving tiles without rentalListing");
                    HashMap::new()
                }
            },
            None => HashMap::new(),
        }
    }

    async fn build(
        &self,
        rental_listings: HashMap<String, TileRentalListing>,
    ) -> anyhow::Result<(MapData, Option<i64>)> {
        let sql = format!(
            r#"
            -- Parcel coordinates are correlated with category. Without isolating the
            -- small estate set, PostgreSQL estimates very few parcels and repeats
            -- the estate primary-key lookup for every one of the 92k parcels.
            WITH estates AS MATERIALIZED (
                SELECT id, name, owner_address, updated_at,
                       search_order_price, search_order_expires_at
                FROM {schema}.nft
                WHERE category = 'estate'
            )
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
                e.search_order_expires_at::int8      AS estate_order_expires_at
            FROM {schema}.nft p
            LEFT JOIN estates e
                   ON e.id = p.search_parcel_estate_id
            WHERE p.category = 'parcel'
              AND p.search_parcel_x IS NOT NULL
              AND p.search_parcel_y IS NOT NULL
            "#,
            schema = self.schema
        );

        let rows = sqlx::query_as::<_, ParcelRow>(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let now_secs = now_ms / 1000;

        let mut tiles: HashMap<String, Tile> =
            HashMap::with_capacity(rows.len() + self.special_tiles.len());
        let mut last_updated_at: i64 = 0;
        let mut next_order_expiry: Option<i64> = None;

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

            if let Some((price, expires_secs)) =
                live_order_price(price_str.as_deref(), expires, now_secs)
            {
                tile.price = Some(price);
                tile.expires_at = Some(expires_secs);
                next_order_expiry =
                    Some(next_order_expiry.map_or(expires_secs, |t| t.min(expires_secs)));
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
                    rental_listing: None,
                },
            );
        }

        let mut estates_owned: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        let mut estates_all: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        for t in tiles.values() {
            if let Some(eid) = &t.estate_id {
                estates_all.entry(eid.clone()).or_default().push((t.x, t.y));
                if t.tile_type == TileType::Owned {
                    estates_owned
                        .entry(eid.clone())
                        .or_default()
                        .push((t.x, t.y));
                }
            }
        }

        Ok((
            MapData {
                tiles,
                last_updated_at,
                estates_owned,
                estates_all,
            },
            next_order_expiry,
        ))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lazy_component(tiles_cap: usize, png_cap: usize) -> MapComponent {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://u:p@127.0.0.1:5999/db")
            .unwrap();
        MapComponent::new(
            pool,
            "s".into(),
            "0xland".into(),
            "0xestate".into(),
            tiles_cap,
            png_cap,
        )
    }

    #[tokio::test]
    async fn gen_cache_bounded_within_epoch() {
        let mc = lazy_component(8, 8);
        for i in 0..100u32 {
            mc.store_tiles_response(format!("k{i}"), Arc::new(vec![i as u8]));
        }
        assert_eq!(
            mc.tiles_cache_len(),
            8,
            "cache must be bounded to cap, not grow to N"
        );
    }

    const FORCE: Duration = Duration::from_secs(900);

    fn fp(rows: i64, max_updated_at: i64, rentals: u64) -> GridFingerprint {
        GridFingerprint {
            rows,
            max_updated_at,
            rentals,
        }
    }

    fn stamp(
        fingerprint: Option<GridFingerprint>,
        age: Duration,
        next_order_expiry: Option<i64>,
    ) -> (BuildStamp, Instant) {
        let now = Instant::now();
        (
            BuildStamp {
                fingerprint,
                built_at: now - age,
                next_order_expiry,
            },
            now,
        )
    }

    #[test]
    fn first_refresh_always_builds() {
        let cur = fp(1, 1, 0);
        assert_eq!(
            rebuild_reason(None, Some(&cur), Instant::now(), 0, FORCE),
            Some(RebuildReason::Initial)
        );
        assert_eq!(
            rebuild_reason(None, None, Instant::now(), 0, FORCE),
            Some(RebuildReason::Initial)
        );
    }

    #[test]
    fn unchanged_fingerprint_skips() {
        let (prev, now) = stamp(
            Some(fp(99_109, 1_789_660_187, 0)),
            Duration::from_secs(60),
            None,
        );
        let cur = fp(99_109, 1_789_660_187, 0);
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&cur), now, 1_789_660_300, FORCE),
            None
        );
    }

    #[test]
    fn any_fingerprint_field_moving_rebuilds() {
        let (prev, now) = stamp(Some(fp(10, 100, 7)), Duration::from_secs(60), None);
        for cur in [
            fp(11, 100, 7),
            fp(10, 101, 7),
            fp(10, 99, 7),
            fp(10, 100, 8),
        ] {
            assert_eq!(
                rebuild_reason(Some(&prev), Some(&cur), now, 0, FORCE),
                Some(RebuildReason::FingerprintChanged),
                "{cur:?}"
            );
        }
    }

    #[test]
    fn failed_fingerprint_rebuilds_like_before() {
        let (prev, now) = stamp(Some(fp(10, 100, 0)), Duration::from_secs(1), None);
        assert_eq!(
            rebuild_reason(Some(&prev), None, now, 0, FORCE),
            Some(RebuildReason::FingerprintUnavailable)
        );
        let (prev, now) = stamp(None, Duration::from_secs(1), None);
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&fp(10, 100, 0)), now, 0, FORCE),
            Some(RebuildReason::FingerprintChanged),
            "a build made without a fingerprint cannot vouch for the next one"
        );
    }

    #[test]
    fn priced_order_expiry_forces_rebuild() {
        let (prev, now) = stamp(Some(fp(10, 100, 0)), Duration::from_secs(60), Some(1_000));
        let cur = fp(10, 100, 0);
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&cur), now, 999, FORCE),
            None
        );
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&cur), now, 1_000, FORCE),
            Some(RebuildReason::OrderExpired)
        );
    }

    #[test]
    fn force_interval_is_the_safety_net() {
        let cur = fp(10, 100, 0);
        let (prev, now) = stamp(Some(fp(10, 100, 0)), FORCE - Duration::from_secs(1), None);
        assert_eq!(rebuild_reason(Some(&prev), Some(&cur), now, 0, FORCE), None);
        let (prev, now) = stamp(Some(fp(10, 100, 0)), FORCE, None);
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&cur), now, 0, FORCE),
            Some(RebuildReason::ForceInterval)
        );
    }

    #[test]
    fn change_wins_over_force_and_expiry() {
        let (prev, now) = stamp(Some(fp(10, 100, 0)), FORCE * 2, Some(1));
        assert_eq!(
            rebuild_reason(Some(&prev), Some(&fp(10, 101, 0)), now, 5, FORCE),
            Some(RebuildReason::FingerprintChanged)
        );
    }

    fn listing(expiration: i64, updated_at: i64, price: &str) -> TileRentalListing {
        TileRentalListing {
            expiration,
            periods: vec![crate::rentals::RentalPeriod {
                min_days: 1,
                max_days: 7,
                price_per_day: price.into(),
            }],
            updated_at,
        }
    }

    #[test]
    fn rentals_hash_is_order_independent_and_content_sensitive() {
        assert_eq!(rentals_hash(&HashMap::new()), 0);
        let a: HashMap<String, TileRentalListing> = [
            ("n1".to_string(), listing(10, 1, "5")),
            ("n2".to_string(), listing(20, 2, "6")),
        ]
        .into_iter()
        .collect();
        let b: HashMap<String, TileRentalListing> = [
            ("n2".to_string(), listing(20, 2, "6")),
            ("n1".to_string(), listing(10, 1, "5")),
        ]
        .into_iter()
        .collect();
        assert_eq!(rentals_hash(&a), rentals_hash(&b));
        let mut c = a.clone();
        c.insert("n2".into(), listing(20, 2, "7"));
        assert_ne!(rentals_hash(&a), rentals_hash(&c));
        let mut d = a.clone();
        d.remove("n1");
        assert_ne!(rentals_hash(&a), rentals_hash(&d));
        let mut e = a.clone();
        e.insert("n1".into(), listing(11, 1, "5"));
        assert_ne!(rentals_hash(&a), rentals_hash(&e));
    }

    #[test]
    fn live_order_price_handles_seconds_and_millis() {
        let now = 1_700_000_000;
        assert_eq!(
            live_order_price(Some("2000000000000000000000"), Some(1_700_000_010), now),
            Some((2000.0, 1_700_000_010))
        );
        assert_eq!(
            live_order_price(Some("2000000000000000000000"), Some(1_700_000_010_499), now),
            Some((2000.0, 1_700_000_010))
        );
        assert_eq!(live_order_price(Some("1"), Some(1_700_000_000), now), None);
        assert_eq!(
            live_order_price(Some("1"), Some(1_699_999_999_000), now),
            None
        );
        assert_eq!(
            live_order_price(Some("abc"), Some(1_700_000_010), now),
            None
        );
        assert_eq!(live_order_price(None, Some(1_700_000_010), now), None);
        assert_eq!(live_order_price(Some("1"), None, now), None);
    }

    #[test]
    fn fingerprint_sql_matches_the_partial_index_predicate() {
        let sql = fingerprint_sql("squid_marketplace");
        println!("{sql}");
        assert!(sql.contains("FROM squid_marketplace.nft"));
        assert!(sql.contains("category IN ('parcel', 'estate')"));
        assert!(sql.contains("count(*)"));
        assert!(sql.contains("max(updated_at)"));
    }

    #[tokio::test]
    async fn force_rebuild_after_is_configurable() {
        let mc = lazy_component(1, 1).with_force_rebuild_after(Duration::from_secs(5));
        assert_eq!(mc.force_rebuild_after, Duration::from_secs(5));
        assert!(mc.build_stamp().is_none());
    }
}
