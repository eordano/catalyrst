//! Direct port of `marketplace-server/src/ports/activity/{component,types}.ts`
//! plus the `adapters/activity` helpers.
//!
//! Composes sales, bids, orders, trades — we only fan-out into the in-crate
//! ports that exist today (sales, bids, orders). The trades port lives here
//! alongside and exposes `get_trades_by_address`; the activity component
//! pulls from all four.

use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::http::response::ApiError;
use crate::ports::bids::{BidFilters, BidsComponent};
use crate::ports::orders::{OrderFilters, OrdersComponent};
use crate::ports::sales::{SaleFilters, SalesComponent};
use crate::ports::trades::TradesComponent;

const INTERNAL_FETCH_CAP: i64 = 500;
const MAX_PAGE_SIZE: i64 = 500;

#[derive(Debug, Serialize)]
pub struct ActivityEvent {
    pub id: String,
    pub timestamp: i64,
    pub network: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "contractAddress", skip_serializing_if = "Option::is_none")]
    pub contract_address: Option<String>,
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    #[serde(rename = "itemId", skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<String>,
    pub details: Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ActivityOptions {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub struct ActivityComponent {
    pub sales: Arc<SalesComponent>,
    pub bids: Arc<BidsComponent>,
    pub orders: Arc<OrdersComponent>,
    pub trades: Arc<TradesComponent>,
}

impl ActivityComponent {
    pub fn new(
        sales: Arc<SalesComponent>,
        bids: Arc<BidsComponent>,
        orders: Arc<OrdersComponent>,
        trades: Arc<TradesComponent>,
    ) -> Self {
        Self { sales, bids, orders, trades }
    }

    pub async fn get_user_activity(
        &self,
        address: &str,
        options: ActivityOptions,
    ) -> Result<(Vec<ActivityEvent>, i64), ApiError> {
        let requested = options.limit.filter(|v| *v > 0).unwrap_or(MAX_PAGE_SIZE);
        let limit = requested.min(MAX_PAGE_SIZE);
        let offset = options.offset.filter(|v| *v > 0).unwrap_or(0);
        let per = INTERNAL_FETCH_CAP;
        let lower = address.to_lowercase();

        // Each source is fetched independently; if any fails we degrade
        // gracefully so a transient failure on one source doesn't 500 the
        // whole activity response (mirrors the safeFetch helper upstream).
        let (sales_buyer, _) = self
            .sales
            .get_sales(&SaleFilters {
                buyer: Some(lower.clone()),
                first: Some(per),
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        let (sales_seller, _) = self
            .sales
            .get_sales(&SaleFilters {
                seller: Some(lower.clone()),
                first: Some(per),
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        let (bids_bidder, _) = self
            .bids
            .get_bids(&BidFilters {
                bidder: Some(lower.clone()),
                limit: per,
                offset: 0,
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        let (bids_seller, _) = self
            .bids
            .get_bids(&BidFilters {
                seller: Some(lower.clone()),
                limit: per,
                offset: 0,
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        let (orders_owner, _) = self
            .orders
            .get_orders(&OrderFilters {
                owner: Some(lower.clone()),
                first: Some(per),
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        let (orders_buyer, _) = self
            .orders
            .get_orders(&OrderFilters {
                buyer: Some(lower.clone()),
                first: Some(per),
                ..Default::default()
            })
            .await
            .unwrap_or_default();

        let mut events: Vec<ActivityEvent> = Vec::new();
        for s in &sales_buyer {
            events.push(sale_to_event(s, "sale_buyer"));
        }
        for s in &sales_seller {
            events.push(sale_to_event(s, "sale_seller"));
        }
        for b in &bids_bidder {
            events.push(bid_to_event(b, "bid_placed"));
        }
        for b in &bids_seller {
            if b.bidder.to_lowercase() != lower {
                events.push(bid_to_event(b, "bid_received"));
            }
        }
        for o in &orders_owner {
            events.push(order_to_event(o, "order_created"));
        }
        for o in &orders_buyer {
            if o.status == "sold" {
                events.push(order_to_event(o, "order_filled"));
            }
        }

        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        // dedup by (id, type)
        let mut seen = std::collections::HashSet::new();
        events.retain(|e| seen.insert((e.id.clone(), e.event_type.clone())));
        let total = events.len() as i64;
        let page: Vec<ActivityEvent> = events
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok((page, total))
    }
}

fn sale_to_event(s: &crate::ports::sales::Sale, ty: &str) -> ActivityEvent {
    ActivityEvent {
        id: s.id.clone(),
        timestamp: s.timestamp,
        network: serde_json::to_string(&s.network).unwrap_or_default().trim_matches('"').to_string(),
        event_type: ty.to_string(),
        contract_address: Some(s.contract_address.clone()),
        token_id: s.token_id.clone(),
        item_id: s.item_id.clone(),
        price: Some(s.price.clone()),
        counterparty: if ty == "sale_buyer" {
            Some(s.seller.clone())
        } else {
            Some(s.buyer.clone())
        },
        details: json!({ "sale": s }),
    }
}

fn bid_to_event(b: &crate::ports::bids::Bid, ty: &str) -> ActivityEvent {
    ActivityEvent {
        id: b.id.clone(),
        timestamp: b.created_at,
        network: serde_json::to_string(&b.network).unwrap_or_default().trim_matches('"').to_string(),
        event_type: ty.to_string(),
        contract_address: Some(b.contract_address.clone()),
        token_id: b.token_id.clone(),
        item_id: b.item_id.clone(),
        price: Some(b.price.clone()),
        counterparty: if ty == "bid_placed" {
            Some(b.seller.clone())
        } else {
            Some(b.bidder.clone())
        },
        details: json!({ "bid": b }),
    }
}

fn order_to_event(o: &crate::ports::orders::Order, ty: &str) -> ActivityEvent {
    ActivityEvent {
        id: o.id.clone(),
        timestamp: o.created_at as i64,
        network: serde_json::to_string(&o.network).unwrap_or_default().trim_matches('"').to_string(),
        event_type: ty.to_string(),
        contract_address: Some(o.contract_address.clone()),
        token_id: o.token_id.clone(),
        item_id: None,
        price: Some(o.price.clone()),
        counterparty: if ty == "order_created" {
            o.buyer.clone()
        } else {
            Some(o.owner.clone())
        },
        details: json!({ "order": o }),
    }
}
