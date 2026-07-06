use sqlx::postgres::PgPool;
use sqlx::Row;

use crate::http::ApiError;

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

pub const ORDER_SCAN_MAX_PAGES: usize = 20;

pub const QUOTE_ORDER_SCAN_MAX_PAGES: usize = 5;

#[derive(Clone)]
pub struct PricingClient {
    http: reqwest::Client,
    market_base_url: String,
    price_base_url: String,
    markup_bps: i64,
    max_staleness_secs: i64,
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
        }
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

    pub async fn fetch_mana_usd(&self) -> Result<String, ApiError> {
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
        let info = self.fetch_item(collection, item_id).await?;
        let open_listing = if mode == "secondary" || mode == "auto" {
            self.fetch_open_listing_scanning(&info.contract_address, item_id, max_pages, true)
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
        let basis = self.fetch_charge_basis(collection, item_id, mode).await?;
        let mana_usd = self.fetch_mana_usd().await?;
        let credit_price = self
            .compute_credit_price(pool, &basis.basis_wei, &mana_usd)
            .await?;
        ensure_charge_covers_payment(&basis.basis_wei, &credit_price)?;
        Ok(PricedItem {
            basis,
            credit_price,
        })
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
        const PAGE: usize = 100;

        let url = format!("{}/v1/orders", self.market_base_url);
        let now = chrono::Utc::now().timestamp();

        for page in 0..max_pages {
            let first = PAGE.to_string();
            let skip = (page * PAGE).to_string();
            let resp = self
                .http
                .get(&url)
                .query(&[
                    ("contractAddress", collection),
                    ("status", "open"),
                    ("sortBy", "cheapest"),
                    ("first", first.as_str()),
                    ("skip", skip.as_str()),
                ])
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

            if let Some(listing) = select_cheapest_listing(orders, item_id, now, include_trades) {
                return Ok(Some(listing));
            }

            if orders.len() < PAGE {
                return Ok(None);
            }
        }

        tracing::warn!(
            collection,
            item_id,
            scanned = max_pages * PAGE,
            "fetch_open_listing: page cap hit with no item-matching listing; \
             failing CLOSED (collection may have more open orders than scanned)"
        );
        Ok(None)
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
                "no open marketplace listing to fulfil this item from — it may have just been bought",
            )),
        },
        "primary" => {
            if mintable_for_charge {
                Ok(mint())
            } else if info.store_mintable {
                Err(ApiError::conflict(
                    "this item mints for free — free mints aren't sold through checkout",
                ))
            } else {
                Err(ApiError::conflict(
                    "this item's mint is not available from its collection store right now — \
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
                 collection store right now — it may have just sold out",
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

pub fn charge_is_positive(credit_price: &str) -> bool {
    let s = credit_price.trim();
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return false;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    int_part.bytes().chain(frac_part.bytes()).any(|b| b != b'0')
}

pub fn payment_is_positive(basis_wei: &str) -> bool {
    let s = basis_wei.trim();
    s.is_empty() || !s.bytes().all(|b| b == b'0')
}

pub fn ensure_charge_covers_payment(basis_wei: &str, credit_price: &str) -> Result<(), ApiError> {
    if payment_is_positive(basis_wei) && !charge_is_positive(credit_price) {
        return Err(ApiError::conflict(
            "this item cannot be safely priced in Credits right now — please try again later",
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
            "// This file was generated by catalyrst-credits \
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
mod tests {
    use super::*;

    #[test]
    fn credit_peg_is_ten_credits_per_usd() {
        assert_eq!(CREDIT_USD, "0.10");
        let v: f64 = CREDIT_USD.parse().unwrap();
        assert!((v - 0.10).abs() < 1e-12);
        assert_eq!((1.0 / v).round() as i64, 10);
    }

    #[test]
    fn fresh_reading_not_stale() {
        assert!(!is_stale(1_000, 1_300, 300));
        assert!(!is_stale(1_000, 1_000, 300));
    }

    #[test]
    fn old_reading_is_stale() {
        assert!(is_stale(1_000, 1_301, 300));
    }

    #[test]
    fn future_reading_is_not_stale() {
        assert!(!is_stale(2_000, 1_000, 300));
    }

    #[test]
    fn extreme_values_do_not_panic() {
        assert!(!is_stale(i64::MIN, i64::MIN, 300));
        assert!(is_stale(i64::MIN, i64::MAX, 300));
        assert!(!is_stale(i64::MAX, i64::MIN, 300));
    }

    #[test]
    fn item_query_uses_both_collection_and_index() {
        let params = item_query_params("0x59a90bad9570ecd08895f132daf7b79696337f61", "0");
        assert_eq!(
            params,
            [
                (
                    "contractAddress",
                    "0x59a90bad9570ecd08895f132daf7b79696337f61"
                ),
                ("itemId", "0"),
            ]
        );
    }

    #[test]
    fn json_as_i64_accepts_int_and_float() {
        assert_eq!(
            json_as_i64(&serde_json::json!(1_690_000_000_i64)),
            Some(1_690_000_000)
        );
        assert_eq!(
            json_as_i64(&serde_json::json!(1_690_000_000.0)),
            Some(1_690_000_000)
        );
        assert_eq!(json_as_i64(&serde_json::json!("nope")), None);
    }

    const TWO_POW_216: &str = "105312291668557186697918027683670432318895095400549111254310977536";

    #[test]
    fn token_matches_item_decodes_dcl_v2_encoding() {
        assert!(token_matches_item("2901", "0"));
        assert!(!token_matches_item("2901", "1"));
        let item1_issued7 = (alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
            + alloy_primitives::U256::from(7u64))
        .to_string();
        assert!(token_matches_item(&item1_issued7, "1"));
        assert!(!token_matches_item(&item1_issued7, "0"));
        assert!(!token_matches_item("", "0"));
        assert!(!token_matches_item("0x2901", "0"));
    }

    fn order(mkt: &str, token: &str, price: &str, status: &str, expires: i64) -> serde_json::Value {
        serde_json::json!({
            "marketplaceAddress": mkt,
            "tokenId": token,
            "price": price,
            "status": status,
            "expiresAt": expires,
        })
    }

    fn trade_order(
        trade_id: &str,
        token: &str,
        price: &str,
        status: &str,
        expires: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": trade_id,
            "tradeId": trade_id,
            "marketplaceAddress": "0x540fb08eDb56AaE562864B390542C97F562825BA",
            "tokenId": token,
            "price": price,
            "status": status,
            "expiresAt": expires,
        })
    }

    fn v2_of(l: &OpenListing) -> (&str, &str) {
        match &l.venue {
            ListingVenue::V2 { token_id } => (token_id, &l.price_wei),
            other => panic!("expected a V2 listing, got {other:?}"),
        }
    }

    #[test]
    fn select_cheapest_listing_picks_cheapest_matching_marketplacev2() {
        let mv2 = MARKETPLACE_V2_POLYGON;
        let now = 1_000i64;
        let orders = vec![
            order(
                "0xa40b1d129b8906888720686f3a01921ddf37716f",
                "2460",
                "1",
                "open",
                9_999,
            ),
            order(
                mv2,
                &(alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
                    + alloy_primitives::U256::from(3u64))
                .to_string(),
                "5",
                "open",
                9_999,
            ),
            order(mv2, "2900", "5", "open", 500),
            order(mv2, "2902", "20000000000000000", "open", 9_999),
            order(mv2, "2901", "10000000000000000", "open", 9_999),
        ];
        let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
        assert_eq!(v2_of(&got), ("2901", "10000000000000000"));
    }

    #[test]
    fn select_cheapest_listing_accepts_trade_venue_pinned_by_trade_id() {
        let now = 1_000i64;
        let orders = vec![
            order(
                MARKETPLACE_V2_POLYGON,
                "2901",
                "20000000000000000",
                "open",
                9_999,
            ),
            trade_order(
                "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
                "2902",
                "10000000000000000",
                "open",
                9_999,
            ),
        ];
        let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
        assert_eq!(
            got.venue,
            ListingVenue::Trade {
                trade_id: "1bbe7d78-dd71-4cbe-9085-70d679d3ad11".into()
            }
        );
        assert_eq!(got.price_wei, "10000000000000000");
    }

    #[test]
    fn price_tie_prefers_the_onchain_v2_listing_over_the_trade() {
        let now = 1_000i64;
        let orders = vec![
            trade_order(
                "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
                "2902",
                "10000000000000000",
                "open",
                9_999,
            ),
            order(
                MARKETPLACE_V2_POLYGON,
                "2901",
                "10000000000000000",
                "open",
                9_999,
            ),
        ];
        let got = select_cheapest_listing(&orders, "0", now, true).expect("a match");
        assert!(
            matches!(got.venue, ListingVenue::V2 { .. }),
            "tie must go to on-chain V2: {got:?}"
        );
    }

    #[test]
    fn trades_are_excluded_when_the_caller_cannot_fulfil_them() {
        let now = 1_000i64;
        let orders = vec![trade_order(
            "1bbe7d78-dd71-4cbe-9085-70d679d3ad11",
            "2902",
            "10000000000000000",
            "open",
            9_999,
        )];
        assert!(select_cheapest_listing(&orders, "0", now, false).is_none());
    }

    #[test]
    fn expired_or_closed_trades_are_not_candidates() {
        let now = 1_000i64;
        let expired = vec![trade_order("t-1", "2902", "1000", "open", 500)];
        assert!(select_cheapest_listing(&expired, "0", now, true).is_none());
        let sold = vec![trade_order("t-1", "2902", "1000", "sold", 9_999)];
        assert!(select_cheapest_listing(&sold, "0", now, true).is_none());
    }

    fn info(price_wei: &str) -> ItemInfo {
        ItemInfo {
            item_id: "0".into(),
            urn: "urn:decentraland:matic:collections-v2:0x59a9:0".into(),
            category: "wearable".into(),
            price_wei: price_wei.into(),
            contract_address: "0x59a90bad9570ecd08895f132daf7b79696337f61".into(),
            store_mintable: false,
        }
    }

    fn mintable_info(price_wei: &str) -> ItemInfo {
        ItemInfo {
            store_mintable: true,
            ..info(price_wei)
        }
    }

    #[test]
    fn secondary_basis_is_the_selected_listing_not_the_mint_price() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "10000000000000000".into(),
        };
        let got = resolve_basis("secondary", &info("0"), Some(listing)).unwrap();
        assert_eq!(got.basis_wei, "10000000000000000");
        assert_eq!(
            got.kind,
            BasisKind::Secondary {
                token_id: "2901".into()
            }
        );
    }

    #[test]
    fn secondary_without_listing_is_rejected_not_priced_at_mint() {
        let err = resolve_basis("secondary", &info("0"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
        let err = resolve_basis("secondary", &info("2500000000000000000"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[test]
    fn primary_basis_stays_the_mint_price() {
        let got = resolve_basis("primary", &mintable_info("2500000000000000000"), None).unwrap();
        assert_eq!(got.basis_wei, "2500000000000000000");
        assert_eq!(got.kind, BasisKind::Primary);
    }

    #[test]
    fn primary_refuses_when_the_store_cannot_mint() {
        let err = resolve_basis("primary", &info("2500000000000000000"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "10000000000000000".into(),
        };
        let err =
            resolve_basis("primary", &info("2500000000000000000"), Some(listing)).unwrap_err();
        assert!(
            matches!(err, ApiError::Conflict(_)),
            "primary mode must not silently fall back to a listing: {err:?}"
        );
    }

    #[test]
    fn unknown_mode_is_refused_not_defaulted_to_mint() {
        let err = resolve_basis("tertiary", &mintable_info("1"), None).unwrap_err();
        assert!(matches!(err, ApiError::Internal(_)), "got {err:?}");
        let err = resolve_basis("", &mintable_info("1"), None).unwrap_err();
        assert!(matches!(err, ApiError::Internal(_)), "got {err:?}");
    }

    #[test]
    fn auto_picks_the_cheaper_listing_over_the_pricier_mint() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "10000000000000000".into(),
        };
        let got =
            resolve_basis("auto", &mintable_info("2500000000000000000"), Some(listing)).unwrap();
        assert_eq!(got.basis_wei, "10000000000000000");
        assert_eq!(
            got.kind,
            BasisKind::Secondary {
                token_id: "2901".into()
            }
        );
    }

    #[test]
    fn auto_picks_the_cheaper_mint_over_the_pricier_listing() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "2500000000000000000".into(),
        };
        let got =
            resolve_basis("auto", &mintable_info("10000000000000000"), Some(listing)).unwrap();
        assert_eq!(got.basis_wei, "10000000000000000");
        assert_eq!(got.kind, BasisKind::Primary);
    }

    #[test]
    fn auto_ignores_a_free_mint_and_charges_the_listing() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "1".into(),
        };
        let got = resolve_basis("auto", &mintable_info("0"), Some(listing)).unwrap();
        assert_eq!(got.basis_wei, "1");
        assert_eq!(
            got.kind,
            BasisKind::Secondary {
                token_id: "2901".into()
            }
        );
    }

    #[test]
    fn auto_refuses_a_free_mint_with_no_listing() {
        let err = resolve_basis("auto", &mintable_info("0"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[test]
    fn primary_refuses_a_free_mint() {
        let err = resolve_basis("primary", &mintable_info("0"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[test]
    fn auto_price_tie_goes_to_the_listing() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "10000000000000000".into(),
        };
        let got =
            resolve_basis("auto", &mintable_info("10000000000000000"), Some(listing)).unwrap();
        assert_eq!(
            got.kind,
            BasisKind::Secondary {
                token_id: "2901".into()
            },
            "on a tie the existing listing is bought, not a new mint"
        );
    }

    #[test]
    fn auto_uses_the_listing_when_the_store_cannot_mint_even_if_mint_looks_cheaper() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "2500000000000000000".into(),
        };
        let got = resolve_basis("auto", &info("10000000000000000"), Some(listing)).unwrap();
        assert_eq!(got.basis_wei, "2500000000000000000");
        assert!(matches!(got.kind, BasisKind::Secondary { .. }));
    }

    #[test]
    fn auto_unparseable_mint_price_defers_to_the_listing() {
        let listing = OpenListing {
            venue: ListingVenue::V2 {
                token_id: "2901".into(),
            },
            price_wei: "10000000000000000".into(),
        };
        let got = resolve_basis("auto", &mintable_info("not-a-price"), Some(listing)).unwrap();
        assert!(matches!(got.kind, BasisKind::Secondary { .. }));
        assert!(!mint_undercuts_listing("not-a-price", "10000000000000000"));
        assert!(!mint_undercuts_listing("1", "garbage"));
        assert!(mint_undercuts_listing(" 1 ", "2"));
        assert!(!mint_undercuts_listing("2", "2"));
    }

    #[test]
    fn auto_falls_back_to_the_store_mint_when_no_listing() {
        let got = resolve_basis("auto", &mintable_info("2500000000000000000"), None).unwrap();
        assert_eq!(got.basis_wei, "2500000000000000000");
        assert_eq!(got.kind, BasisKind::Primary);
    }

    #[test]
    fn auto_refuses_when_neither_listing_nor_store_mint_exists() {
        let err = resolve_basis("auto", &info("2500000000000000000"), None).unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[test]
    fn item_mintability_comes_from_is_on_sale_without_a_trade() {
        let mintable = |is_on_sale: bool, trade_id: serde_json::Value| {
            let on_sale = is_on_sale;
            let has_trade = trade_id.as_str().is_some_and(|s| !s.trim().is_empty());
            on_sale && !has_trade
        };
        assert!(mintable(true, serde_json::Value::Null));
        assert!(mintable(true, serde_json::json!("")));
        assert!(!mintable(true, serde_json::json!("df638de9-uuid")));
        assert!(!mintable(false, serde_json::Value::Null));
    }

    #[test]
    fn quote_and_checkout_derive_the_same_basis_from_the_same_selection() {
        let now = 1_000i64;
        let orders = vec![
            order(
                MARKETPLACE_V2_POLYGON,
                "2902",
                "20000000000000000",
                "open",
                9_999,
            ),
            order(
                MARKETPLACE_V2_POLYGON,
                "2901",
                "10000000000000000",
                "open",
                9_999,
            ),
        ];
        let quote_side = resolve_basis(
            "secondary",
            &info("0"),
            select_cheapest_listing(&orders, "0", now, true),
        )
        .unwrap();
        let checkout_side = resolve_basis(
            "secondary",
            &info("0"),
            select_cheapest_listing(&orders, "0", now, true),
        )
        .unwrap();
        assert_eq!(quote_side, checkout_side);
        assert_eq!(quote_side.basis_wei, "10000000000000000");
    }

    #[test]
    fn never_zero_guard_rejects_zero_charge_while_broker_pays() {
        let err = ensure_charge_covers_payment("10000000000000000", "0").unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
        assert!(ensure_charge_covers_payment("10000000000000000", "abc").is_err());
        assert!(ensure_charge_covers_payment("10000000000000000", "").is_err());
        assert!(ensure_charge_covers_payment("garbage", "0").is_err());
        assert!(ensure_charge_covers_payment("", "0").is_err());
        ensure_charge_covers_payment("10000000000000000", "1").unwrap();
        ensure_charge_covers_payment("0", "0").unwrap();
        ensure_charge_covers_payment("000", "0.00").unwrap();
        ensure_charge_covers_payment("0", "5").unwrap();
    }

    #[test]
    fn charge_and_payment_positivity_parsers() {
        assert!(charge_is_positive("1"));
        assert!(charge_is_positive("0.5"));
        assert!(charge_is_positive(" 10 "));
        assert!(!charge_is_positive("0"));
        assert!(!charge_is_positive("0.000"));
        assert!(!charge_is_positive(""));
        assert!(!charge_is_positive("-1"));
        assert!(!charge_is_positive("1e3"));

        assert!(payment_is_positive("1"));
        assert!(payment_is_positive("10000000000000000"));
        assert!(payment_is_positive("nonsense"));
        assert!(payment_is_positive(""));
        assert!(!payment_is_positive("0"));
        assert!(!payment_is_positive("000"));
    }

    #[test]
    fn select_cheapest_listing_none_when_no_matching_venue() {
        let now = 1_000i64;
        let orders = vec![order(
            "0xa40b1d129b8906888720686f3a01921ddf37716f",
            "2460",
            "1",
            "open",
            9_999,
        )];
        assert!(select_cheapest_listing(&orders, "0", now, true).is_none());
        let orders2 = vec![order(
            MARKETPLACE_V2_POLYGON,
            &(alloy_primitives::U256::from_str_radix(TWO_POW_216, 10).unwrap()
                + alloy_primitives::U256::from(1u64))
            .to_string(),
            "1",
            "open",
            9_999,
        )];
        assert!(select_cheapest_listing(&orders2, "0", now, true).is_none());
    }
}
