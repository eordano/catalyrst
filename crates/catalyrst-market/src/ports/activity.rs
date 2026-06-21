use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::http::response::ApiError;
use crate::ports::bids::{BidFilters, BidsComponent};
use crate::ports::orders::{OrderFilters, OrdersComponent};
use crate::ports::sales::{SaleFilters, SalesComponent};
use crate::ports::trades::{TradeView, TradesComponent};

const INTERNAL_FETCH_CAP: i64 = 500;
const MAX_PAGE_SIZE: i64 = 500;
const ASSET_TYPE_ERC20: i32 = 1;
const ASSET_TYPE_USD_PEGGED_MANA: i32 = 2;

#[derive(Debug, Serialize)]
pub struct ActivityEvent {
    pub id: String,
    pub timestamp: i64,
    pub network: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "txHash", skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
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
        Self {
            sales,
            bids,
            orders,
            trades,
        }
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

        let f_sales_buyer = SaleFilters {
            buyer: Some(lower.clone()),
            first: Some(per),
            ..Default::default()
        };
        let f_sales_seller = SaleFilters {
            seller: Some(lower.clone()),
            first: Some(per),
            ..Default::default()
        };
        let f_bids_bidder = BidFilters {
            bidder: Some(lower.clone()),
            limit: per,
            offset: 0,
            ..Default::default()
        };
        let f_bids_seller = BidFilters {
            seller: Some(lower.clone()),
            limit: per,
            offset: 0,
            ..Default::default()
        };
        let f_orders_owner = OrderFilters {
            owner: Some(lower.clone()),
            first: Some(per),
            ..Default::default()
        };
        let f_orders_buyer = OrderFilters {
            buyer: Some(lower.clone()),
            first: Some(per),
            ..Default::default()
        };
        let (sales_buyer, sales_seller, bids_bidder, bids_seller, orders_owner, orders_buyer) = tokio::join!(
            self.sales.get_sales(&f_sales_buyer),
            self.sales.get_sales(&f_sales_seller),
            self.bids.get_bids(&f_bids_bidder),
            self.bids.get_bids(&f_bids_seller),
            self.orders.get_orders(&f_orders_owner),
            self.orders.get_orders(&f_orders_buyer),
        );
        let (sales_buyer, _) = sales_buyer.unwrap_or_default();
        let (sales_seller, _) = sales_seller.unwrap_or_default();
        let (bids_bidder, _) = bids_bidder.unwrap_or_default();
        let (bids_seller, _) = bids_seller.unwrap_or_default();
        let (orders_owner, _) = orders_owner.unwrap_or_default();
        let (orders_buyer, _) = orders_buyer.unwrap_or_default();
        let user_trades = self
            .trades
            .get_trades_by_address(&lower, per, 0)
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
        for t in &user_trades {
            events.push(trade_to_event(t));
        }

        events.sort_by_key(|b| std::cmp::Reverse(b.timestamp));

        let mut seen = std::collections::HashSet::new();
        events.retain(|e| seen.insert(dedup_key(e)));
        let total = events.len() as i64;
        let page: Vec<ActivityEvent> = events
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok((page, total))
    }
}

fn dedup_key(e: &ActivityEvent) -> String {
    match &e.tx_hash {
        Some(tx) => format!("tx:{}:{}", tx.to_lowercase(), e.event_type),
        None => format!(
            "{}|{}|{}|{}",
            e.contract_address.as_deref().unwrap_or("-"),
            e.token_id
                .as_deref()
                .or(e.item_id.as_deref())
                .unwrap_or("-"),
            e.timestamp,
            e.event_type
        ),
    }
}

fn sale_to_event(s: &crate::ports::sales::Sale, ty: &str) -> ActivityEvent {
    ActivityEvent {
        id: format!("{ty}:{}", s.id),
        timestamp: s.timestamp,
        network: serde_json::to_string(&s.network)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
        event_type: ty.to_string(),
        tx_hash: Some(s.tx_hash.clone()),
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
        id: format!("{ty}:{}", b.id),
        timestamp: b.created_at,
        network: serde_json::to_string(&b.network)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
        event_type: ty.to_string(),
        tx_hash: None,
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
        id: format!("{ty}:{}", o.id),
        timestamp: o.created_at,
        network: serde_json::to_string(&o.network)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
        event_type: ty.to_string(),
        tx_hash: None,
        contract_address: Some(o.contract_address.clone()),
        token_id: o.token_id.clone(),
        item_id: None,
        price: Some(o.price.clone()),
        counterparty: if ty == "order_created" {
            None
        } else {
            Some(o.owner.clone())
        },
        details: json!({ "order": o }),
    }
}

fn trade_to_event(t: &TradeView) -> ActivityEvent {
    let is_payment = |a: &&crate::ports::trades::TradeAsset| {
        a.asset_type == ASSET_TYPE_ERC20 || a.asset_type == ASSET_TYPE_USD_PEGGED_MANA
    };
    let assets = t.sent.iter().chain(t.received.iter());
    let non_payment = assets.clone().find(|a| !is_payment(a));
    let payment = assets.clone().find(is_payment);
    ActivityEvent {
        id: format!("trade_created:{}", t.trade.id),
        timestamp: t.trade.created_at.timestamp_millis(),
        network: t.trade.network.clone(),
        event_type: "trade_created".to_string(),
        tx_hash: None,
        contract_address: non_payment.map(|a| a.contract_address.clone()),
        token_id: non_payment.and_then(|a| a.token_id.clone()),
        item_id: non_payment.and_then(|a| a.item_id.clone()),
        price: payment.and_then(|a| a.amount.clone()),
        counterparty: None,
        details: json!({ "trade": t }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::trades::{DbTrade, TradeAsset, TradeView};
    use chrono::TimeZone;
    use sqlx::types::JsonValue;

    fn asset(asset_type: i32, token_id: Option<&str>, amount: Option<&str>) -> TradeAsset {
        TradeAsset {
            asset_type,
            contract_address: "0xnft".into(),
            beneficiary: None,
            direction: "sent".into(),
            extra: String::new(),
            amount: amount.map(|s| s.to_string()),
            token_id: token_id.map(|s| s.to_string()),
            item_id: None,
        }
    }

    fn trade(sent: Vec<TradeAsset>, received: Vec<TradeAsset>) -> TradeView {
        let ts = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        TradeView {
            trade: DbTrade {
                id: "t1".into(),
                chain_id: 1,
                checks: JsonValue::Null,
                created_at: ts,
                effective_since: ts,
                expires_at: ts,
                network: "ETHEREUM".into(),
                signature: String::new(),
                signer: "0xsigner".into(),
                trade_type: "public_nft_order".into(),
                contract: String::new(),
            },
            sent,
            received,
        }
    }

    #[test]
    fn trade_event_picks_non_payment_and_payment() {
        let t = trade(
            vec![asset(3, Some("42"), None)],
            vec![asset(1, None, Some("1000"))],
        );
        let ev = trade_to_event(&t);
        assert_eq!(ev.id, "trade_created:t1");
        assert_eq!(ev.event_type, "trade_created");
        assert_eq!(ev.contract_address.as_deref(), Some("0xnft"));
        assert_eq!(ev.token_id.as_deref(), Some("42"));
        assert_eq!(ev.price.as_deref(), Some("1000"));
        assert_eq!(ev.counterparty, None);
        assert_eq!(ev.timestamp, 1_700_000_000_000);
    }

    #[test]
    fn trade_event_treats_usd_pegged_mana_as_payment() {
        let t = trade(
            vec![asset(2, None, Some("500"))],
            vec![asset(4, None, None)],
        );
        let ev = trade_to_event(&t);
        assert_eq!(ev.price.as_deref(), Some("500"));
        assert_eq!(ev.contract_address.as_deref(), Some("0xnft"));
    }

    #[test]
    fn trade_event_has_no_tx_hash() {
        let t = trade(
            vec![asset(3, Some("1"), None)],
            vec![asset(1, None, Some("9"))],
        );
        assert_eq!(trade_to_event(&t).tx_hash, None);
    }

    fn ev(
        event_type: &str,
        tx_hash: Option<&str>,
        contract: Option<&str>,
        token: Option<&str>,
        item: Option<&str>,
        timestamp: i64,
    ) -> ActivityEvent {
        ActivityEvent {
            id: format!("{event_type}:x"),
            timestamp,
            network: "ETHEREUM".into(),
            event_type: event_type.into(),
            tx_hash: tx_hash.map(|s| s.to_string()),
            contract_address: contract.map(|s| s.to_string()),
            token_id: token.map(|s| s.to_string()),
            item_id: item.map(|s| s.to_string()),
            price: None,
            counterparty: None,
            details: json!({}),
        }
    }

    #[test]
    fn dedup_key_uses_lowercased_txhash_and_type_when_present() {
        let e = ev(
            "sale_buyer",
            Some("0xABCdef"),
            Some("0xnft"),
            Some("42"),
            None,
            100,
        );
        assert_eq!(dedup_key(&e), "tx:0xabcdef:sale_buyer");
    }

    #[test]
    fn dedup_key_keeps_buyer_and_seller_of_same_tx_distinct() {
        let buyer = ev("sale_buyer", Some("0xdead"), None, None, None, 1);
        let seller = ev("sale_seller", Some("0xdead"), None, None, None, 1);
        assert_ne!(dedup_key(&buyer), dedup_key(&seller));
    }

    #[test]
    fn dedup_key_falls_back_to_composite_without_txhash() {
        let e = ev("bid_placed", None, Some("0xNFT"), None, Some("7"), 55);
        assert_eq!(dedup_key(&e), "0xNFT|7|55|bid_placed");
        let bare = ev("trade_created", None, None, None, None, 9);
        assert_eq!(dedup_key(&bare), "-|-|9|trade_created");
    }

    #[test]
    fn sale_event_threads_and_serializes_tx_hash() {
        use crate::dcl_schemas::{ChainId, Network};
        use crate::ports::sales::Sale;
        let sale = Sale {
            id: "s1".into(),
            item_id: None,
            contract_address: "0xnft".into(),
            buyer: "0xbuyer".into(),
            chain_id: ChainId::EthereumMainnet,
            network: Network::Ethereum,
            price: "1000".into(),
            seller: "0xseller".into(),
            timestamp: 100,
            token_id: Some("42".into()),
            tx_hash: "0xTXHASH".into(),
            sale_type: "order".into(),
        };
        let e = sale_to_event(&sale, "sale_buyer");
        assert_eq!(e.tx_hash.as_deref(), Some("0xTXHASH"));
        let j = serde_json::to_value(&e).unwrap();
        assert_eq!(j.get("txHash").and_then(|v| v.as_str()), Some("0xTXHASH"));
    }
}
