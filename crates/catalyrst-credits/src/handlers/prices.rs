use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::Json;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use crate::handlers::cart::{validate_collection, validate_item_id};
use crate::http::ApiError;
use crate::ports::pricing::{ensure_charge_covers_payment, QUOTE_ORDER_SCAN_MAX_PAGES};
use crate::AppState;

const MAX_ENTRIES: usize = 60;

const QUOTE_BATCH_CONCURRENCY: usize = 8;

pub const QUOTE_CACHE_TTL: Duration = Duration::from_secs(60);

pub const QUOTE_CACHE_MAX_ENTRIES: usize = 10_000;

type QuoteCacheMap = HashMap<(String, String), (Instant, Option<String>)>;

pub struct QuoteCache {
    ttl: Duration,
    max_entries: usize,
    inner: Mutex<QuoteCacheMap>,
}

impl QuoteCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries,
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, collection: &str, item_id: &str) -> Option<Option<String>> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let (at, credits) = map.get(&(collection.to_string(), item_id.to_string()))?;
        (at.elapsed() < self.ttl).then(|| credits.clone())
    }

    pub fn put(&self, collection: &str, item_id: &str, credits: Option<String>) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() >= self.max_entries {
            let ttl = self.ttl;
            map.retain(|_, (at, _)| at.elapsed() < ttl);
            if map.len() >= self.max_entries {
                map.clear();
            }
        }
        map.insert(
            (collection.to_string(), item_id.to_string()),
            (Instant::now(), credits),
        );
    }
}

impl Default for QuoteCache {
    fn default() -> Self {
        Self::new(QUOTE_CACHE_TTL, QUOTE_CACHE_MAX_ENTRIES)
    }
}

#[derive(Debug, Deserialize)]
pub struct QuoteItemRef {
    #[serde(rename = "itemId")]
    item_id: String,
    collection: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct QuoteBody {
    #[serde(default)]
    items: Vec<QuoteItemRef>,

    #[serde(default)]
    amounts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct ItemQuoteOut {
    #[serde(rename = "itemId")]
    item_id: String,
    collection: String,
    credits: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "credits/"))]
pub struct PriceQuotesOut {
    items: Vec<ItemQuoteOut>,
    amounts: Vec<Option<String>>,
}

fn valid_wei(raw: &str) -> Option<&str> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 30 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s)
}

pub async fn quote(
    State(state): State<AppState>,
    body: Json<QuoteBody>,
) -> Result<Json<PriceQuotesOut>, ApiError> {
    let Json(body) = body;
    if body.items.is_empty() && body.amounts.is_empty() {
        return Err(ApiError::bad_request("nothing to quote"));
    }
    if body.items.len() > MAX_ENTRIES || body.amounts.len() > MAX_ENTRIES {
        return Err(ApiError::bad_request(format!(
            "too many entries (max {MAX_ENTRIES} items and {MAX_ENTRIES} amounts)"
        )));
    }

    let mut refs = Vec::with_capacity(body.items.len());
    for r in &body.items {
        refs.push((
            validate_collection(&r.collection)?,
            validate_item_id(&r.item_id)?,
        ));
    }

    let mut quoted: Vec<Option<Option<String>>> = refs
        .iter()
        .map(|(collection, item_id)| state.quote_cache.get(collection, item_id))
        .collect();
    let misses: Vec<usize> = (0..refs.len()).filter(|&i| quoted[i].is_none()).collect();

    let mana_usd = if !misses.is_empty() || !body.amounts.is_empty() {
        Some(state.pricing.fetch_mana_usd().await?)
    } else {
        None
    };

    if let Some(mana_usd) = &mana_usd {
        let jobs: Vec<_> = misses
            .iter()
            .map(|&i| {
                let (collection, item_id) = refs[i].clone();
                let state = state.clone();
                let mana_usd = mana_usd.clone();
                async move {
                    let basis = state
                        .pricing
                        .fetch_charge_basis_scanning(
                            &collection,
                            &item_id,
                            &state.checkout_fulfillment_mode,
                            QUOTE_ORDER_SCAN_MAX_PAGES,
                        )
                        .await
                        .ok()?;
                    let credits = state
                        .pricing
                        .compute_credit_price(&state.credits.pool, &basis.basis_wei, &mana_usd)
                        .await
                        .ok()?;
                    ensure_charge_covers_payment(&basis.basis_wei, &credits).ok()?;
                    Some(credits)
                }
            })
            .collect();
        let fresh: Vec<Option<String>> = stream::iter(jobs)
            .buffered(QUOTE_BATCH_CONCURRENCY)
            .collect()
            .await;

        for (&i, credits) in misses.iter().zip(fresh) {
            let (collection, item_id) = &refs[i];
            state.quote_cache.put(collection, item_id, credits.clone());
            quoted[i] = Some(credits);
        }
    }

    let items = refs
        .into_iter()
        .zip(quoted)
        .map(|((collection, item_id), credits)| ItemQuoteOut {
            item_id,
            collection,
            credits: credits.flatten(),
        })
        .collect();

    let mut amounts = Vec::with_capacity(body.amounts.len());
    for raw in &body.amounts {
        let credits = match (valid_wei(raw), mana_usd.as_ref()) {
            (Some(wei), Some(mana_usd)) => state
                .pricing
                .compute_credit_price(&state.credits.pool, wei, mana_usd)
                .await
                .ok(),
            _ => None,
        };
        amounts.push(credits);
    }

    Ok(Json(PriceQuotesOut { items, amounts }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_identity_price_quotes() {
        let out = PriceQuotesOut {
            items: vec![
                ItemQuoteOut {
                    item_id: "12".into(),
                    collection: "0x59a90bad9570ecd08895f132daf7b79696337f61".into(),
                    credits: Some("2".into()),
                },
                ItemQuoteOut {
                    item_id: "3".into(),
                    collection: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    credits: None,
                },
            ],
            amounts: vec![Some("1".into()), None],
        };
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!({
                "items": [
                    {
                        "itemId": "12",
                        "collection": "0x59a90bad9570ecd08895f132daf7b79696337f61",
                        "credits": "2",
                    },
                    {
                        "itemId": "3",
                        "collection": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "credits": null,
                    },
                ],
                "amounts": ["1", null],
            })
        );
    }

    #[test]
    fn quote_body_defaults_are_empty() {
        let b: QuoteBody = serde_json::from_value(json!({})).unwrap();
        assert!(b.items.is_empty());
        assert!(b.amounts.is_empty());
        let b: QuoteBody = serde_json::from_value(json!({
            "items": [{ "itemId": "1", "collection": "0xabc" }],
            "amounts": ["10000000000000000"],
        }))
        .unwrap();
        assert_eq!(b.items.len(), 1);
        assert_eq!(b.amounts, vec!["10000000000000000".to_string()]);
    }

    #[test]
    fn quote_cache_hit_within_ttl_serves_without_refetch() {
        let cache = QuoteCache::new(Duration::from_secs(60), 100);
        assert_eq!(cache.get("0xabc", "1"), None, "cold cache misses");
        cache.put("0xabc", "1", Some("2".into()));
        assert_eq!(cache.get("0xabc", "1"), Some(Some("2".into())));
        cache.put("0xabc", "2", None);
        assert_eq!(cache.get("0xabc", "2"), Some(None));
        assert_eq!(cache.get("0xdef", "1"), None);
    }

    #[test]
    fn quote_cache_expires_after_ttl() {
        let cache = QuoteCache::new(Duration::ZERO, 100);
        cache.put("0xabc", "1", Some("2".into()));
        assert_eq!(cache.get("0xabc", "1"), None, "zero TTL expires instantly");
    }

    #[test]
    fn quote_cache_stays_bounded() {
        let cache = QuoteCache::new(Duration::from_secs(60), 2);
        cache.put("0xabc", "1", Some("1".into()));
        cache.put("0xabc", "2", Some("2".into()));
        cache.put("0xabc", "3", Some("3".into()));
        assert_eq!(cache.get("0xabc", "3"), Some(Some("3".into())));
        assert_eq!(cache.get("0xabc", "1"), None);
        assert_eq!(cache.get("0xabc", "2"), None);
    }

    #[test]
    fn wei_validation() {
        assert_eq!(valid_wei(" 10000000000000000 "), Some("10000000000000000"));
        assert_eq!(valid_wei("0"), Some("0"));
        assert_eq!(valid_wei(""), None);
        assert_eq!(valid_wei("1.5"), None);
        assert_eq!(valid_wei("0x10"), None);
        assert_eq!(valid_wei(&"9".repeat(31)), None);
    }
}
