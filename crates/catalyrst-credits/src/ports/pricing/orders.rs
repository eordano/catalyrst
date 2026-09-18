use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::TtlMap;

use super::{select_cheapest_listing, OpenListing, PricingClient};
use crate::http::ApiError;

const ORDERS_PAGE: usize = super::ORDER_SCAN_PAGE;

pub const QUOTE_ORDERS_PAGE_TTL: Duration = Duration::from_secs(10);

const QUOTE_ORDERS_PAGE_MAX_ENTRIES: usize = 2_000;

pub type OrdersPage = Arc<Vec<serde_json::Value>>;

pub type OrdersPageMemo = Arc<TtlMap<(String, usize), OrdersPage>>;

pub fn new_memo() -> OrdersPageMemo {
    Arc::new(TtlMap::bounded(
        "credits-quote-orders",
        QUOTE_ORDERS_PAGE_TTL,
        QUOTE_ORDERS_PAGE_MAX_ENTRIES,
    ))
}

impl PricingClient {
    pub(super) async fn scan_open_listing(
        &self,
        collection: &str,
        item_id: &str,
        max_pages: usize,
        include_trades: bool,
        memo: bool,
    ) -> Result<Option<OpenListing>, ApiError> {
        let now = chrono::Utc::now().timestamp();

        for page in 0..max_pages {
            let orders = if memo {
                let key = (collection.to_ascii_lowercase(), page);
                self.quote_pages
                    .get_or_fetch(key, || self.fetch_orders_page(collection, page))
                    .await?
            } else {
                self.fetch_orders_page(collection, page).await?
            };

            if let Some(listing) = select_cheapest_listing(&orders, item_id, now, include_trades) {
                return Ok(Some(listing));
            }

            if orders.len() < ORDERS_PAGE {
                return Ok(None);
            }
        }

        tracing::warn!(
            collection,
            item_id,
            scanned = max_pages * ORDERS_PAGE,
            "fetch_open_listing: page cap hit with no item-matching listing; \
             failing CLOSED (collection may have more open orders than scanned)"
        );
        Ok(None)
    }

    async fn fetch_orders_page(
        &self,
        collection: &str,
        page: usize,
    ) -> Result<OrdersPage, ApiError> {
        let url = format!("{}/v1/orders", self.market_base_url);
        let first = ORDERS_PAGE.to_string();
        let skip = (page * ORDERS_PAGE).to_string();
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

        let orders = body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .ok_or_else(|| {
                ApiError::Internal("market orders response missing data array".into())
            })?;
        Ok(Arc::new(orders))
    }
}
