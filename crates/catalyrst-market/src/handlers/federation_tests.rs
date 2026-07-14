use super::*;
use serde_json::json;

#[test]
fn wire_identity_write_ack() {
    let (status, Json(body)) = ok_json("824a4634e2d62f4821ef5730b39111dc".to_string());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::to_value(body).unwrap(),
        json!({ "ok": true, "signature_hash": "824a4634e2d62f4821ef5730b39111dc" })
    );
}

#[test]
fn wire_identity_write_error() {
    let (status, Json(body)) = err_json(StatusCode::NOT_FOUND, "bid not found");
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        serde_json::to_value(body).unwrap(),
        json!({ "ok": false, "message": "bid not found" })
    );
}

#[test]
fn wire_identity_write_error_via_map_apply_err() {
    let (status, Json(body)) = map_apply_err(ApiError::Http(catalyrst_types::HttpError {
        code: 404,
        message: "order not found".to_string(),
    }));
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        serde_json::to_value(body).unwrap(),
        json!({ "ok": false, "message": "order not found" })
    );
}

#[test]
fn wire_identity_snapshot() {
    let dto = MarketSnapshot {
        latest_bids_seq: 35,
        latest_orders_seq: 44,
        latest_trades_seq: 17,
        latest_cancellations_seq: 32,
        latest_acceptances_seq: 0,
        log_hash: "4230d9f8f415e7d5b15060a635b0602060bbdf1796195b8f6f277a7dbe092fae".to_string(),
        domain: "DecentralandMarket",
    };
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "latest_bids_seq":         35,
            "latest_orders_seq":       44,
            "latest_trades_seq":       17,
            "latest_cancellations_seq": 32,
            "latest_acceptances_seq":  0,
            "log_hash":                "4230d9f8f415e7d5b15060a635b0602060bbdf1796195b8f6f277a7dbe092fae",
            "domain":                  "DecentralandMarket",
        })
    );
}

#[test]
fn wire_identity_snapshot_empty_log() {
    let empty_hash = hex::encode(Sha256::new().finalize());
    let dto = MarketSnapshot {
        latest_bids_seq: 0,
        latest_orders_seq: 0,
        latest_trades_seq: 0,
        latest_cancellations_seq: 0,
        latest_acceptances_seq: 0,
        log_hash: empty_hash.clone(),
        domain: "DecentralandMarket",
    };
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "latest_bids_seq":         0,
            "latest_orders_seq":       0,
            "latest_trades_seq":       0,
            "latest_cancellations_seq": 0,
            "latest_acceptances_seq":  0,
            "log_hash":                empty_hash,
            "domain":                  "DecentralandMarket",
        })
    );
}

fn sample_bid_row() -> BidRow {
    (
        "ebe1d8b5027a4e0bac79a61c1d286931".to_string(),
        "urn:decentraland:matic:collections-v2:0x01:0".to_string(),
        "0xf4613258f96a1dadf96fe3dad773c94d211db354".to_string(),
        "1000000000000000000".to_string(),
        1782070334,
        "".to_string(),
        1781983934,
        1,
    )
}

fn sample_order_row() -> OrderRow {
    (
        "ad6029122c8e8531737678e16a648048".to_string(),
        "urn:decentraland:matic:collections-v2:0x01:0".to_string(),
        "0xf4613258f96a1dadf96fe3dad773c94d211db354".to_string(),
        "3000000000000000000".to_string(),
        1782070561,
        1781984164,
        44,
    )
}

fn sample_trade_row() -> TradeRow {
    (
        "7707b9c8704d3c5c68c3dbac797e2e5c".to_string(),
        "ad6029122c8e8531737678e16a648048".to_string(),
        "0xf4613258f96a1dadf96fe3dad773c94d211db354".to_string(),
        "0xabcdef1234567890".to_string(),
        1781984161,
        1781984165,
        17,
    )
}

fn sample_cancel_row() -> CancelRow {
    (
        "c0ffee0000000000000000000000cafe".to_string(),
        "ebe1d8b5027a4e0bac79a61c1d286931".to_string(),
        "bid".to_string(),
        "0xf4613258f96a1dadf96fe3dad773c94d211db354".to_string(),
        1781984200,
        32,
    )
}

fn sample_accept_row() -> AcceptRow {
    (
        "acce9700000000000000000000000001".to_string(),
        "ebe1d8b5027a4e0bac79a61c1d286931".to_string(),
        "0xf4613258f96a1dadf96fe3dad773c94d211db354".to_string(),
        1781984300,
        9,
    )
}

#[test]
fn wire_identity_changes_bid_entry() {
    assert_eq!(
        serde_json::to_value(BidChange::from(sample_bid_row())).unwrap(),
        json!({
            "kind": "bid",
            "signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
            "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
            "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
            "price": "1000000000000000000",
            "expires_at": 1782070334,
            "fingerprint": "",
            "signed_at": 1781983934,
            "seq": 1,
        })
    );
}

#[test]
fn wire_identity_changes_order_entry() {
    assert_eq!(
        serde_json::to_value(OrderChange::from(sample_order_row())).unwrap(),
        json!({
            "kind": "order",
            "signature_hash": "ad6029122c8e8531737678e16a648048",
            "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
            "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
            "price": "3000000000000000000",
            "expires_at": 1782070561,
            "signed_at": 1781984164,
            "seq": 44,
        })
    );
}

#[test]
fn wire_identity_changes_trade_entry() {
    assert_eq!(
        serde_json::to_value(TradeChange::from_row(sample_trade_row())).unwrap(),
        json!({
            "kind": "trade",
            "signature_hash": "7707b9c8704d3c5c68c3dbac797e2e5c",
            "order_signature_hash": "ad6029122c8e8531737678e16a648048",
            "buyer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
            "tx_hash": "0xabcdef1234567890",
            "taken_at": 1781984161,
            "signed_at": 1781984165,
            "seq": 17,
        })
    );
}

#[test]
fn wire_identity_changes_cancel_entry() {
    assert_eq!(
        serde_json::to_value(CancelChange::from(sample_cancel_row())).unwrap(),
        json!({
            "kind": "cancel",
            "signature_hash": "c0ffee0000000000000000000000cafe",
            "target_signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
            "target_kind": "bid",
            "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
            "signed_at": 1781984200,
            "seq": 32,
        })
    );
}

#[test]
fn wire_identity_changes_accept_entry() {
    assert_eq!(
        serde_json::to_value(AcceptChange::from(sample_accept_row())).unwrap(),
        json!({
            "kind": "accept",
            "signature_hash": "acce9700000000000000000000000001",
            "bid_signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
            "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
            "signed_at": 1781984300,
            "seq": 9,
        })
    );
}

#[test]
fn wire_identity_changes_envelope_empty() {
    let dto = MarketChanges {
        bids: vec![],
        orders: vec![],
        trades: vec![],
        cancellations: vec![],
        acceptances: vec![],
    };
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "bids": [],
            "orders": [],
            "trades": [],
            "cancellations": [],
            "acceptances": [],
        })
    );
}

#[test]
fn wire_identity_changes_envelope_populated() {
    let dto = MarketChanges {
        bids: vec![BidChange::from(sample_bid_row())],
        orders: vec![OrderChange::from(sample_order_row())],
        trades: vec![TradeChange::from_row(sample_trade_row())],
        cancellations: vec![CancelChange::from(sample_cancel_row())],
        acceptances: vec![AcceptChange::from(sample_accept_row())],
    };
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "bids": [{
                "kind": "bid",
                "signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
                "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "price": "1000000000000000000",
                "expires_at": 1782070334,
                "fingerprint": "",
                "signed_at": 1781983934,
                "seq": 1,
            }],
            "orders": [{
                "kind": "order",
                "signature_hash": "ad6029122c8e8531737678e16a648048",
                "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "price": "3000000000000000000",
                "expires_at": 1782070561,
                "signed_at": 1781984164,
                "seq": 44,
            }],
            "trades": [{
                "kind": "trade",
                "signature_hash": "7707b9c8704d3c5c68c3dbac797e2e5c",
                "order_signature_hash": "ad6029122c8e8531737678e16a648048",
                "buyer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "tx_hash": "0xabcdef1234567890",
                "taken_at": 1781984161,
                "signed_at": 1781984165,
                "seq": 17,
            }],
            "cancellations": [{
                "kind": "cancel",
                "signature_hash": "c0ffee0000000000000000000000cafe",
                "target_signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
                "target_kind": "bid",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "signed_at": 1781984200,
                "seq": 32,
            }],
            "acceptances": [{
                "kind": "accept",
                "signature_hash": "acce9700000000000000000000000001",
                "bid_signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "signed_at": 1781984300,
                "seq": 9,
            }],
        })
    );
}

#[test]
fn wire_identity_list_bids() {
    let data: Vec<FedBidEntry> = vec![FedBidEntry::from(sample_bid_row())];
    let total = data.len();
    assert_eq!(
        serde_json::to_value(FedList { data, total }).unwrap(),
        json!({
            "data": [{
                "signature_hash": "ebe1d8b5027a4e0bac79a61c1d286931",
                "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "price": "1000000000000000000",
                "expires_at": 1782070334,
                "fingerprint": "",
                "signed_at": 1781983934,
                "seq": 1,
            }],
            "total": 1,
        })
    );
}

#[test]
fn wire_identity_list_orders() {
    let data: Vec<FedOrderEntry> = vec![FedOrderEntry::from(sample_order_row())];
    let total = data.len();
    assert_eq!(
        serde_json::to_value(FedList { data, total }).unwrap(),
        json!({
            "data": [{
                "signature_hash": "ad6029122c8e8531737678e16a648048",
                "item_id": "urn:decentraland:matic:collections-v2:0x01:0",
                "signer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "price": "3000000000000000000",
                "expires_at": 1782070561,
                "signed_at": 1781984164,
                "seq": 44,
            }],
            "total": 1,
        })
    );
}

#[test]
fn wire_identity_list_trades() {
    let data: Vec<FedTradeEntry> = vec![FedTradeEntry::from_row(sample_trade_row())];
    let total = data.len();
    assert_eq!(
        serde_json::to_value(FedList { data, total }).unwrap(),
        json!({
            "data": [{
                "signature_hash": "7707b9c8704d3c5c68c3dbac797e2e5c",
                "order_signature_hash": "ad6029122c8e8531737678e16a648048",
                "buyer": "0xf4613258f96a1dadf96fe3dad773c94d211db354",
                "tx_hash": "0xabcdef1234567890",
                "taken_at": 1781984161,
                "signed_at": 1781984165,
                "seq": 17,
            }],
            "total": 1,
        })
    );
}

#[test]
fn wire_identity_list_empty() {
    let data: Vec<FedBidEntry> = vec![];
    let total = data.len();
    assert_eq!(
        serde_json::to_value(FedList { data, total }).unwrap(),
        json!({ "data": [], "total": 0 })
    );
}
