use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;
use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::http::ApiError;

mod orders;

const ALLOWED_CATEGORIES: [&str; 2] = ["wearable", "emote"];

pub const CREDIT_USD: &str = "0.10";

#[derive(Debug, Clone)]
pub struct ItemInfo {
    pub item_id: String,
    pub urn: String,

    pub category: String,

    pub price_wei: String,

    pub contract_address: String,

    pub store_mintable: bool,
}

#[derive(Debug, Clone)]
pub struct OpenOrder {
    pub token_id: String,
    pub price_wei: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListingVenue {
    V2 { token_id: String },
    Trade { trade_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenListing {
    pub venue: ListingVenue,
    pub price_wei: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasisKind {
    Primary,
    Secondary { token_id: String },
    Trade { trade_id: String },
}

impl BasisKind {
    pub fn is_single_listing(&self) -> bool {
        !matches!(self, BasisKind::Primary)
    }
}

#[derive(Debug, Clone)]
pub struct ChargeBasis {
    pub info: ItemInfo,

    pub basis_wei: String,

    pub kind: BasisKind,
}

#[derive(Debug, Clone)]
pub struct PricedItem {
    pub basis: ChargeBasis,

    pub credit_price: String,
}

const MARKETPLACE_V2_POLYGON: &str = "0x480a0f4e360e8964e68858dd231c2922f1df45ef";

pub const TRADE_CONTRACT_POLYGON: &str = "0x540fb08edb56aae562864b390542c97f562825ba";

pub const ORDER_SCAN_PAGE: usize = 100;

pub const ORDER_SCAN_MAX_PAGES: usize = 20;

pub const QUOTE_ORDER_SCAN_MAX_PAGES: usize = 5;

/// One oracle read serves every quote/cart/checkout/authorize/topup/outbox caller for this long.
pub const MANA_USD_MEMO_TTL: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct PricingClient {
    http: reqwest::Client,
    market_base_url: String,
    price_base_url: String,
    markup_bps: i64,
    max_staleness_secs: i64,
    quote_pages: orders::OrdersPageMemo,
    mana_usd: Arc<TtlMap<(), String>>,
}

impl PricingClient {
    pub fn new(
        http: reqwest::Client,
        market_base_url: String,
        price_base_url: String,
        markup_bps: i64,
        max_staleness_secs: i64,
    ) -> Self {
        Self {
            http,
            market_base_url,
            price_base_url,
            markup_bps,
            max_staleness_secs,
            quote_pages: orders::new_memo(),
            mana_usd: Arc::new(TtlMap::new("credits-mana-usd", MANA_USD_MEMO_TTL)),
        }
    }

    pub fn markup_bps(&self) -> i64 {
        self.markup_bps
    }

    pub async fn fetch_item(&self, collection: &str, item_id: &str) -> Result<ItemInfo, ApiError> {
        let url = format!("{}/v1/items", self.market_base_url);
        let resp = self
            .http
            .get(&url)
            .query(&item_query_params(collection, item_id))
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("market request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            if status.as_u16() == 404 {
                return Err(ApiError::not_found("item not found in catalog"));
            }
            return Err(ApiError::Internal(format!(
                "market returned status {}",
                status.as_u16()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("market response parse failed: {e}")))?;

        let items = body
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ApiError::Internal("market response missing data array".into()))?;

        let item = match items.len() {
            0 => return Err(ApiError::not_found("item not found in catalog")),
            1 => &items[0],
            n => {
                return Err(ApiError::Internal(format!(
                    "catalog returned {n} items for contractAddress={collection}&itemId={item_id} (ambiguous)"
                )))
            }
        };

        parse_item_info(item, item_id)
    }

    /// [`Self::fetch_item`] for many `(collection, item_id)` pairs in ONE `/v1/items?id=..`
    /// round trip; element `i` answers `pairs[i]` exactly as the single lookup would
    /// (not-found, ambiguous, category gate), so callers keep the per-item error semantics.
    pub async fn fetch_items_batch(
        &self,
        pairs: &[(String, String)],
    ) -> Result<Vec<Result<ItemInfo, ApiError>>, ApiError> {
        if pairs.is_empty() {
            return Ok(vec![]);
        }
        let mut keys: Vec<(String, String)> = Vec::new();
        let mut query: Vec<(&str, String)> = Vec::new();
        for (collection, item_id) in pairs {
            let key = (collection.to_ascii_lowercase(), item_id.clone());
            if !keys.contains(&key) {
                query.push(("id", format!("{}-{}", key.0, key.1)));
                keys.push(key);
            }
        }
        // Twice the key count so a duplicated key still shows up as ambiguous.
        query.push(("first", (keys.len() * 2).to_string()));

        let url = format!("{}/v1/items", self.market_base_url);
        let resp = self
            .http
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("market request failed: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            if status.as_u16() == 404 {
                return Ok(pairs
                    .iter()
                    .map(|_| Err(ApiError::not_found("item not found in catalog")))
                    .collect());
            }
            return Err(ApiError::Internal(format!(
                "market returned status {}",
                status.as_u16()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("market response parse failed: {e}")))?;
        let items = body
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ApiError::Internal("market response missing data array".into()))?;

        let mut by_key: HashMap<(String, String), Vec<&serde_json::Value>> = HashMap::new();
        for item in items {
            if let Some(key) = item_key(item) {
                by_key.entry(key).or_default().push(item);
            }
        }
        Ok(pairs
            .iter()
            .map(|(collection, item_id)| {
                let key = (collection.to_ascii_lowercase(), item_id.clone());
                match by_key.get(&key).map(|v| v.as_slice()) {
                    None | Some([]) => Err(ApiError::not_found("item not found in catalog")),
                    Some([item]) => parse_item_info(item, item_id),
                    Some(v) => Err(ApiError::Internal(format!(
                        "catalog returned {} items for contractAddress={collection}&itemId={item_id} (ambiguous)",
                        v.len()
                    ))),
                }
            })
            .collect())
    }

    pub async fn fetch_mana_usd(&self) -> Result<String, ApiError> {
        self.mana_usd
            .get_or_fetch((), || self.fetch_mana_usd_uncached())
            .await
    }

    async fn fetch_mana_usd_uncached(&self) -> Result<String, ApiError> {
        let url = format!("{}/api/v3/simple/price", self.price_base_url);
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("ids", "decentraland"),
                ("vs_currencies", "usd"),
                ("include_last_updated_at", "true"),
            ])
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("price oracle request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ApiError::Internal(format!(
                "price oracle returned status {}",
                resp.status().as_u16()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("price oracle parse failed: {e}")))?;

        let mana = body
            .get("decentraland")
            .ok_or_else(|| ApiError::Internal("oracle missing 'decentraland'".into()))?;

        let usd_value = mana
            .get("usd")
            .filter(|v| v.is_number())
            .ok_or_else(|| ApiError::Internal("oracle missing numeric 'usd'".into()))?;
        let usd = usd_value.to_string();

        let last_updated_at = mana
            .get("last_updated_at")
            .and_then(json_as_i64)
            .ok_or_else(|| ApiError::Internal("oracle missing 'last_updated_at'".into()))?;

        let now = chrono::Utc::now().timestamp();
        if is_stale(last_updated_at, now, self.max_staleness_secs) {
            return Err(ApiError::Internal(format!(
                "MANA/USD oracle is stale (age {}s exceeds {}s)",
                now - last_updated_at,
                self.max_staleness_secs
            )));
        }

        Ok(usd)
    }

    pub async fn compute_credit_price(
        &self,
        pool: &PgPool,
        mana_wei: &str,
        mana_usd: &str,
    ) -> Result<String, ApiError> {
        let row = sqlx::query(
            "SELECT ceil( \
                 ($1::numeric / 1e18) * $2::numeric \
                 * (1 + ($3::numeric / 10000)) \
                 / $4::numeric \
             )::text AS credit_price",
        )
        .bind(mana_wei)
        .bind(mana_usd)
        .bind(self.markup_bps)
        .bind(CREDIT_USD)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<String, _>("credit_price"))
    }

    /// Batched form of [`Self::compute_credit_price`]: reprices many wei amounts
    /// in ONE round trip. The `ceil(...)` expression is byte-identical to the
    /// single-row version and the bind roles/types match, so PostgreSQL evaluates
    /// the same NUMERIC arithmetic per row; `WITH ORDINALITY` + `ORDER BY t.ord`
    /// pin element `i` of the result to the reprice of `weis[i]`.
    pub async fn compute_credit_prices_batch(
        &self,
        pool: &PgPool,
        weis: &[String],
        mana_usd: &str,
    ) -> Result<Vec<String>, ApiError> {
        if weis.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query(
            "SELECT ceil( \
                 (t.wei::numeric / 1e18) * $2::numeric \
                 * (1 + ($3::numeric / 10000)) \
                 / $4::numeric \
             )::text AS credit_price \
             FROM unnest($1::text[]) WITH ORDINALITY AS t(wei, ord) \
             ORDER BY t.ord",
        )
        .bind(weis)
        .bind(mana_usd)
        .bind(self.markup_bps)
        .bind(CREDIT_USD)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("credit_price"))
            .collect())
    }

    pub async fn fetch_charge_basis(
        &self,
        collection: &str,
        item_id: &str,
        mode: &str,
    ) -> Result<ChargeBasis, ApiError> {
        self.fetch_charge_basis_scanning(collection, item_id, mode, ORDER_SCAN_MAX_PAGES)
            .await
    }

    pub async fn fetch_charge_basis_scanning(
        &self,
        collection: &str,
        item_id: &str,
        mode: &str,
        max_pages: usize,
    ) -> Result<ChargeBasis, ApiError> {
        self.charge_basis(collection, item_id, mode, max_pages, false)
            .await
    }

    pub async fn fetch_charge_basis_quote(
        &self,
        collection: &str,
        item_id: &str,
        mode: &str,
    ) -> Result<ChargeBasis, ApiError> {
        self.charge_basis(collection, item_id, mode, QUOTE_ORDER_SCAN_MAX_PAGES, true)
            .await
    }

    async fn charge_basis(
        &self,
        collection: &str,
        item_id: &str,
        mode: &str,
        max_pages: usize,
        memo: bool,
    ) -> Result<ChargeBasis, ApiError> {
        let info = self.fetch_item(collection, item_id).await?;
        let open_listing = if mode == "secondary" || mode == "auto" {
            self.scan_open_listing(&info.contract_address, item_id, max_pages, true, memo)
                .await?
        } else {
            None
        };
        let resolved = resolve_basis(mode, &info, open_listing)?;
        Ok(ChargeBasis {
            info,
            basis_wei: resolved.basis_wei,
            kind: resolved.kind,
        })
    }

    pub async fn price_item_for_mode(
        &self,
        pool: &PgPool,
        collection: &str,
        item_id: &str,
        mode: &str,
    ) -> Result<PricedItem, ApiError> {
        let pairs = [(collection.to_string(), item_id.to_string())];
        self.price_items_for_mode(pool, &pairs, mode, ORDER_SCAN_MAX_PAGES)
            .await?
            .pop()
            .unwrap_or_else(|| Err(ApiError::Internal("empty price batch".into())))
    }

    /// [`Self::fetch_charge_basis_scanning`] for many pairs with the market calls collapsed:
    /// one catalog batch and one `/v1/orders/open-by-items` call. Element `i` answers
    /// `pairs[i]`; a batch-level failure (market unreachable) is the outer error.
    pub async fn fetch_charge_bases_batch(
        &self,
        pairs: &[(String, String)],
        mode: &str,
        max_pages: usize,
    ) -> Result<Vec<Result<ChargeBasis, ApiError>>, ApiError> {
        let infos = self.fetch_items_batch(pairs).await?;
        let wants_listing = mode == "secondary" || mode == "auto";
        let listing_pairs: Vec<(String, String)> = infos
            .iter()
            .zip(pairs)
            .filter_map(|(info, (_, item_id))| {
                info.as_ref()
                    .ok()
                    .map(|info| (info.contract_address.clone(), item_id.clone()))
            })
            .collect();
        let listings = if wants_listing && !listing_pairs.is_empty() {
            self.fetch_open_listings_batch(&listing_pairs, max_pages, true)
                .await?
        } else {
            HashMap::new()
        };
        Ok(infos
            .into_iter()
            .zip(pairs)
            .map(|(info, (_, item_id))| {
                let info = info?;
                let open_listing = if wants_listing {
                    let key = (info.contract_address.to_ascii_lowercase(), item_id.clone());
                    listings.get(&key).cloned().flatten()
                } else {
                    None
                };
                let resolved = resolve_basis(mode, &info, open_listing)?;
                Ok(ChargeBasis {
                    info,
                    basis_wei: resolved.basis_wei,
                    kind: resolved.kind,
                })
            })
            .collect())
    }

    /// [`Self::price_item_for_mode`] for many pairs: the two market batches, one (memoized)
    /// oracle read and one batched credit conversion. Element `i` answers `pairs[i]`.
    pub async fn price_items_for_mode(
        &self,
        pool: &PgPool,
        pairs: &[(String, String)],
        mode: &str,
        max_pages: usize,
    ) -> Result<Vec<Result<PricedItem, ApiError>>, ApiError> {
        let bases = self
            .fetch_charge_bases_batch(pairs, mode, max_pages)
            .await?;
        let weis: Vec<String> = bases
            .iter()
            .filter_map(|b| b.as_ref().ok())
            .map(|b| b.basis_wei.clone())
            .collect();
        let mut prices = if weis.is_empty() {
            Vec::new()
        } else {
            let mana_usd = self.fetch_mana_usd().await?;
            self.compute_credit_prices_batch(pool, &weis, &mana_usd)
                .await?
        }
        .into_iter();
        Ok(bases
            .into_iter()
            .map(|basis| {
                let basis = basis?;
                let credit_price = prices.next().ok_or_else(|| {
                    ApiError::Internal("price batch shorter than its input".into())
                })?;
                ensure_charge_covers_payment(&basis.basis_wei, &credit_price)?;
                Ok(PricedItem {
                    basis,
                    credit_price,
                })
            })
            .collect())
    }

    pub async fn fetch_open_order(
        &self,
        collection: &str,
        item_id: &str,
    ) -> Result<Option<OpenOrder>, ApiError> {
        let listing = self
            .fetch_open_listing_scanning(collection, item_id, ORDER_SCAN_MAX_PAGES, false)
            .await?;
        Ok(listing.and_then(|l| match l.venue {
            ListingVenue::V2 { token_id } => Some(OpenOrder {
                token_id,
                price_wei: l.price_wei,
            }),
            ListingVenue::Trade { .. } => None,
        }))
    }

    pub async fn fetch_open_listing_scanning(
        &self,
        collection: &str,
        item_id: &str,
        max_pages: usize,
        include_trades: bool,
    ) -> Result<Option<OpenListing>, ApiError> {
        self.scan_open_listing(collection, item_id, max_pages, include_trades, false)
            .await
    }

    /// [`Self::fetch_open_listing_scanning`] for many `(collection, item_id)` pairs in one
    /// market round trip (`/v1/orders/open-by-items`). The market ranks each collection's open
    /// orders cheapest-first and cuts the first `max_pages * ORDER_SCAN_PAGE` of them, the same
    /// rows the page walk would have fetched, so [`select_cheapest_listing`] sees what it
    /// would have seen; pairs whose item id is not decimal never match a token and resolve to
    /// `None` without asking. Every requested pair is present in the result.
    pub async fn fetch_open_listings_batch(
        &self,
        pairs: &[(String, String)],
        max_pages: usize,
        include_trades: bool,
    ) -> Result<HashMap<(String, String), Option<OpenListing>>, ApiError> {
        let mut out: HashMap<(String, String), Option<OpenListing>> = HashMap::new();
        let mut query: Vec<(&str, String)> = Vec::new();
        for (collection, item_id) in pairs {
            let key = (collection.to_ascii_lowercase(), item_id.clone());
            if is_decimal_item_id(item_id) && !out.contains_key(&key) {
                query.push(("item", format!("{}-{}", key.0, key.1)));
            }
            out.entry(key).or_insert(None);
        }
        if query.is_empty() {
            return Ok(out);
        }
        query.push(("perContract", (max_pages * ORDER_SCAN_PAGE).to_string()));

        let url = format!("{}/v1/orders/open-by-items", self.market_base_url);
        let resp = self
            .http
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("market orders request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::Internal(format!(
                "market orders returned status {}",
                resp.status().as_u16()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("market orders parse failed: {e}")))?;
        let orders = body.get("data").and_then(|d| d.as_array()).ok_or_else(|| {
            ApiError::Internal("market orders response missing data array".into())
        })?;

        let now = chrono::Utc::now().timestamp();
        for (key, slot) in out.iter_mut() {
            *slot = select_listing_for_pair(orders, &key.0, &key.1, now, include_trades);
        }
        Ok(out)
    }

    pub async fn fetch_trade(&self, trade_id: &str) -> Result<serde_json::Value, ApiError> {
        let id = trade_id.trim();
        if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        {
            return Err(ApiError::Internal(format!(
                "invalid trade id {trade_id:?} pinned on the line"
            )));
        }
        let url = format!("{}/v1/trades/{}", self.market_base_url, id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("market trade request failed: {e}")))?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Err(ApiError::not_found("trade not found in the market book"));
        }
        if !status.is_success() {
            return Err(ApiError::Internal(format!(
                "market trade returned status {}",
                status.as_u16()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("market trade parse failed: {e}")))?;
        body.get("data")
            .cloned()
            .ok_or_else(|| ApiError::Internal("market trade response missing data".into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBasis {
    pub basis_wei: String,
    pub kind: BasisKind,
}

pub fn resolve_basis(
    mode: &str,
    info: &ItemInfo,
    open_listing: Option<OpenListing>,
) -> Result<ResolvedBasis, ApiError> {
    let listing = |l: OpenListing| ResolvedBasis {
        basis_wei: l.price_wei,
        kind: match l.venue {
            ListingVenue::V2 { token_id } => BasisKind::Secondary { token_id },
            ListingVenue::Trade { trade_id } => BasisKind::Trade { trade_id },
        },
    };
    let mint = || ResolvedBasis {
        basis_wei: info.price_wei.clone(),
        kind: BasisKind::Primary,
    };
    let mintable_for_charge = info.store_mintable && payment_is_positive(&info.price_wei);
    match mode {
        "secondary" => match open_listing {
            Some(l) => Ok(listing(l)),
            None => Err(ApiError::conflict(
                "no open marketplace listing to fulfil this item from \u{2014} it may have just been bought",
            )),
        },
        "primary" => {
            if mintable_for_charge {
                Ok(mint())
            } else if info.store_mintable {
                Err(ApiError::conflict(
                    "this item mints for free \u{2014} free mints aren't sold through checkout",
                ))
            } else {
                Err(ApiError::conflict(
                    "this item's mint is not available from its collection store right now \u{2014} \
                     it may be off sale or sold out",
                ))
            }
        }
        "auto" => match (open_listing, mintable_for_charge) {
            (Some(l), true) => {
                if mint_undercuts_listing(&info.price_wei, &l.price_wei) {
                    Ok(mint())
                } else {
                    Ok(listing(l))
                }
            }
            (Some(l), false) => Ok(listing(l)),
            (None, true) => Ok(mint()),
            (None, false) => Err(ApiError::conflict(
                "this item has no open marketplace listing and isn't mintable from its \
                 collection store right now \u{2014} it may have just sold out",
            )),
        },
        other => Err(ApiError::Internal(format!(
            "unsupported fulfillment mode {other:?} (expected \"secondary\", \"primary\", or \"auto\")"
        ))),
    }
}

fn mint_undercuts_listing(mint_wei: &str, listing_wei: &str) -> bool {
    match (
        mint_wei.trim().parse::<u128>(),
        listing_wei.trim().parse::<u128>(),
    ) {
        (Ok(mint), Ok(listing)) => mint < listing,
        _ => false,
    }
}

/// Shared prefix of `charge_is_positive` (below) and `parse_nonneg_decimal`
/// (ports/checkout.rs): trim whitespace, split on the first `.`, and accept
/// only `[digits][.[digits]]` with at least one span non-empty. Returns the
/// two spans unmodified for each caller's own tail -- it does not itself
/// decide positivity, magnitude, or sign. Deliberately narrower than
/// `CreditAmount` (crate::money): no sign, no exponent, no magnitude bound.
/// Do NOT widen this grammar to match any other validator in this crate --
/// see the `characterization_*` tests for the documented divergences.
pub(crate) fn split_validated_decimal(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((int_part, frac_part))
}

pub fn charge_is_positive(credit_price: &str) -> bool {
    let Some((int_part, frac_part)) = split_validated_decimal(credit_price) else {
        return false;
    };
    int_part.bytes().chain(frac_part.bytes()).any(|b| b != b'0')
}

pub fn payment_is_positive(basis_wei: &str) -> bool {
    let s = basis_wei.trim();
    s.is_empty() || !s.bytes().all(|b| b == b'0')
}

pub fn ensure_charge_covers_payment(basis_wei: &str, credit_price: &str) -> Result<(), ApiError> {
    if payment_is_positive(basis_wei) && !charge_is_positive(credit_price) {
        return Err(ApiError::conflict(
            "this item cannot be safely priced in Credits right now \u{2014} please try again later",
        ));
    }
    Ok(())
}

fn venue_rank(venue: &ListingVenue) -> u8 {
    match venue {
        ListingVenue::V2 { .. } => 0,
        ListingVenue::Trade { .. } => 1,
    }
}

fn select_cheapest_listing(
    orders: &[serde_json::Value],
    item_id: &str,
    now: i64,
    include_trades: bool,
) -> Option<OpenListing> {
    let mut best: Option<(u128, OpenListing)> = None;
    for o in orders {
        let mkt = o
            .get("marketplaceAddress")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let is_v2 = mkt.eq_ignore_ascii_case(MARKETPLACE_V2_POLYGON);
        let is_trade = include_trades && mkt.eq_ignore_ascii_case(TRADE_CONTRACT_POLYGON);
        if !is_v2 && !is_trade {
            continue;
        }
        if o.get("status").and_then(|v| v.as_str()) != Some("open") {
            continue;
        }
        let expires = o.get("expiresAt").and_then(json_as_i64).unwrap_or(0);
        if expires != 0 && expires <= now {
            continue;
        }
        let token_id = match o.get("tokenId").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => continue,
        };
        if !token_matches_item(&token_id, item_id) {
            continue;
        }
        let venue = if is_v2 {
            ListingVenue::V2 { token_id }
        } else {
            let trade_id = match o
                .get("tradeId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .or_else(|| o.get("id").and_then(|v| v.as_str()))
            {
                Some(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => continue,
            };
            ListingVenue::Trade { trade_id }
        };
        let price_wei = match o.get("price") {
            Some(serde_json::Value::String(s)) if !s.is_empty() => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        let price = match price_wei.parse::<u128>() {
            Ok(p) if p > 0 => p,
            _ => continue,
        };
        let better = match &best {
            None => true,
            Some((bp, bl)) => {
                price < *bp || (price == *bp && venue_rank(&venue) < venue_rank(&bl.venue))
            }
        };
        if better {
            best = Some((price, OpenListing { venue, price_wei }));
        }
    }
    best.map(|(_, listing)| listing)
}

/// One pair's answer out of a mixed-collection order list: only the orders of `collection`
/// are candidates, then [`select_cheapest_listing`] decides exactly as it does for a page.
fn select_listing_for_pair(
    orders: &[serde_json::Value],
    collection: &str,
    item_id: &str,
    now: i64,
    include_trades: bool,
) -> Option<OpenListing> {
    let mine: Vec<serde_json::Value> = orders
        .iter()
        .filter(|o| {
            o.get("contractAddress")
                .and_then(|v| v.as_str())
                .is_some_and(|c| c.eq_ignore_ascii_case(collection))
        })
        .cloned()
        .collect();
    select_cheapest_listing(&mine, item_id, now, include_trades)
}

fn is_decimal_item_id(item_id: &str) -> bool {
    let s = item_id.trim();
    !s.is_empty() && s.len() <= 77 && s.bytes().all(|b| b.is_ascii_digit())
}

fn token_matches_item(token_id: &str, item_id: &str) -> bool {
    use alloy_primitives::U256;
    const ISSUED_ID_BITS: usize = 216;
    let (token_id, item_id) = (token_id.trim(), item_id.trim());
    if token_id.is_empty() || item_id.is_empty() {
        return false;
    }
    let (Ok(tok), Ok(item)) = (
        U256::from_str_radix(token_id, 10),
        U256::from_str_radix(item_id, 10),
    ) else {
        return false;
    };
    (tok >> ISSUED_ID_BITS) == item
}

fn parse_item_info(item: &serde_json::Value, item_id: &str) -> Result<ItemInfo, ApiError> {
    let category = item
        .get("category")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Internal("catalog item missing category".into()))?
        .to_string();
    if !ALLOWED_CATEGORIES.contains(&category.as_str()) {
        return Err(ApiError::bad_request(format!(
            "item category '{category}' is not purchasable (wearable/emote only)"
        )));
    }

    let price_wei = match item.get("price") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => return Err(ApiError::Internal("catalog item missing price".into())),
    };

    let urn = item
        .get("urn")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Internal("catalog item missing urn".into()))?
        .to_string();

    let contract_address = item
        .get("contractAddress")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Internal("catalog item missing contractAddress".into()))?
        .to_string();

    let is_on_sale = item
        .get("isOnSale")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let has_open_trade = item
        .get("tradeId")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());

    Ok(ItemInfo {
        item_id: item_id.to_string(),
        urn,
        category,
        price_wei,
        contract_address,
        store_mintable: is_on_sale && !has_open_trade,
    })
}

/// `(lowercased contractAddress, itemId)` of a catalog row; `itemId` falls back to the
/// `<contract>-<itemId>` suffix of `id` for rows that omit it.
fn item_key(item: &serde_json::Value) -> Option<(String, String)> {
    let contract = item
        .get("contractAddress")
        .and_then(|v| v.as_str())?
        .to_ascii_lowercase();
    let item_id = match item.get("itemId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            let id = item.get("id").and_then(|v| v.as_str())?;
            let (prefix, rest) = id.split_at_checked(contract.len() + 1)?;
            if !prefix[..contract.len()].eq_ignore_ascii_case(&contract) || !prefix.ends_with('-') {
                return None;
            }
            rest.to_string()
        }
    };
    Some((contract, item_id))
}

fn item_query_params<'a>(collection: &'a str, item_id: &'a str) -> [(&'static str, &'a str); 2] {
    [("contractAddress", collection), ("itemId", item_id)]
}

fn json_as_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

pub fn is_stale(last_updated_at: i64, now: i64, max_secs: i64) -> bool {
    (now as i128 - last_updated_at as i128) > max_secs as i128
}

#[cfg(all(test, feature = "ts"))]
mod ts_peg_export {
    use super::CREDIT_USD;

    #[test]
    fn export_bindings_credit_peg() {
        let credit_usd: f64 = CREDIT_USD.parse().expect("CREDIT_USD must be a decimal");
        let credits_per_usd = (1.0 / credit_usd).round() as i64;
        assert!(
            (credits_per_usd as f64 * credit_usd - 1.0).abs() < 1e-9,
            "CREDIT_USD must divide 1 USD into a whole number of Credits"
        );
        let content = format!(
            "// This file was generated by the credits crate \
             (`export_bindings_credit_peg` in ports/pricing.rs). Do not edit this file manually.\n\
             // The hardcoded Credits peg: 1 Credit = {CREDIT_USD} USDC.\n\
             \n\
             export const CREDIT_USD = {credit_usd};\n\
             export const CREDIT_USD_DECIMAL = \"{CREDIT_USD}\";\n\
             export const CREDITS_PER_USD = {credits_per_usd};\n",
        );
        let dir = std::env::var("TS_RS_EXPORT_DIR").unwrap_or_else(|_| "./bindings".into());
        let dir = std::path::Path::new(&dir).join("credits");
        std::fs::create_dir_all(&dir).expect("create export dir");
        std::fs::write(dir.join("CreditPeg.ts"), content).expect("write CreditPeg.ts");
    }
}

#[cfg(test)]
mod tests;

/// Characterizes `charge_is_positive` and `payment_is_positive` on the
/// shared edge-input set used across all decimal-string validators in this
/// crate (see the sibling `characterization_*` tests in money.rs,
/// ports/checkout.rs, purchase_intent.rs, and handlers/packs.rs).
///
/// `charge_is_positive` shares its accept/reject grammar exactly with
/// `parse_nonneg_decimal` in ports/checkout.rs (both call
/// `split_validated_decimal`): scientific notation and a stray extra `.` are
/// rejected, surrounding whitespace is tolerated (unlike `CreditAmount`), and
/// there is no magnitude bound.
///
/// `payment_is_positive` is a DIFFERENT, much looser function: it operates on
/// raw wei integer strings, not Credits decimals, and only rejects a string
/// that is all `'0'` bytes after trimming, so malformed input like `"1e18"`
/// or `"1.2.3"` reads as "positive".
#[cfg(test)]
mod characterization_charge_and_payment_positivity {
    use super::{charge_is_positive, payment_is_positive};

    #[test]
    fn current_accept_reject_on_edge_inputs() {
        assert!(!charge_is_positive("1e18"));
        assert!(!charge_is_positive("1E18"));
        assert!(charge_is_positive(" 1.5 "));
        assert!(charge_is_positive(".5"));
        assert!(charge_is_positive("5."));
        assert!(charge_is_positive("01.50"));
        assert!(!charge_is_positive(""));
        assert!(!charge_is_positive("-1"));
        assert!(!charge_is_positive("1.2.3"));
        assert!(
            charge_is_positive(&"9".repeat(50)),
            "no magnitude bound here, unlike CreditAmount"
        );

        for s in ["1e18", "1E18", " 1.5 ", ".5", "5.", "01.50", "-1", "1.2.3"] {
            assert!(payment_is_positive(s), "{s:?} must read as positive");
        }
        assert!(payment_is_positive(""), "empty reads as positive too");
        assert!(payment_is_positive(&"9".repeat(50)));
    }
}
