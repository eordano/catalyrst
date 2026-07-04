use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_fed::{FedError, RateLimitDecision, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth_chain::require_signer;
use crate::fed::apply;
use crate::fed::authority::{
    lookup_bid_item_id, lookup_bid_signer, lookup_order_signer, order_exists,
    signer_has_active_lease_for_item, signer_owns_any_nft_for_item,
};
use crate::fed::messages::{BidAccept, BidCancel, BidPlace, OrderCancel, OrderCreate, TradeRecord};
use crate::http::response::ApiError;
use crate::AppState;

type BidRow = (String, String, String, String, i64, String, i64, i64);
type OrderRow = (String, String, String, String, i64, i64, i64);
type TradeRow = (String, String, String, String, i64, i64, i64);
type CancelRow = (String, String, String, String, i64, i64);
type AcceptRow = (String, String, String, i64, i64);

#[derive(Debug, Clone, Serialize)]
pub struct FedAck {
    pub ok: bool,
    pub signature_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FedErrorBody {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FedWriteBody {
    Ack(FedAck),
    Error(FedErrorBody),
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketSnapshot {
    pub latest_bids_seq: i64,
    pub latest_orders_seq: i64,
    pub latest_trades_seq: i64,
    pub latest_cancellations_seq: i64,
    pub latest_acceptances_seq: i64,
    pub log_hash: String,
    pub domain: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketChanges {
    pub bids: Vec<BidChange>,
    pub orders: Vec<OrderChange>,
    pub trades: Vec<TradeChange>,
    pub cancellations: Vec<CancelChange>,
    pub acceptances: Vec<AcceptChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BidChange {
    pub kind: &'static str,
    pub signature_hash: String,
    pub item_id: String,
    pub signer: String,
    pub price: String,
    pub expires_at: i64,
    pub fingerprint: String,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<BidRow> for BidChange {
    fn from(
        (signature_hash, item_id, signer, price, expires_at, fingerprint, signed_at, seq): BidRow,
    ) -> Self {
        Self {
            kind: "bid",
            signature_hash,
            item_id,
            signer,
            price,
            expires_at,
            fingerprint,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderChange {
    pub kind: &'static str,
    pub signature_hash: String,
    pub item_id: String,
    pub signer: String,
    pub price: String,
    pub expires_at: i64,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<OrderRow> for OrderChange {
    fn from(
        (signature_hash, item_id, signer, price, expires_at, signed_at, seq): OrderRow,
    ) -> Self {
        Self {
            kind: "order",
            signature_hash,
            item_id,
            signer,
            price,
            expires_at,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeChange {
    pub kind: &'static str,
    pub signature_hash: String,
    pub order_signature_hash: String,
    pub buyer: String,
    pub tx_hash: String,
    pub taken_at: i64,
    pub signed_at: i64,
    pub seq: i64,
}

impl TradeChange {
    fn from_row(
        (signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq): TradeRow,
    ) -> Self {
        Self {
            kind: "trade",
            signature_hash,
            order_signature_hash,
            buyer,
            tx_hash,
            taken_at,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CancelChange {
    pub kind: &'static str,
    pub signature_hash: String,
    pub target_signature_hash: String,
    pub target_kind: String,
    pub signer: String,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<CancelRow> for CancelChange {
    fn from(
        (signature_hash, target_signature_hash, target_kind, signer, signed_at, seq): CancelRow,
    ) -> Self {
        Self {
            kind: "cancel",
            signature_hash,
            target_signature_hash,
            target_kind,
            signer,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AcceptChange {
    pub kind: &'static str,
    pub signature_hash: String,
    pub bid_signature_hash: String,
    pub signer: String,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<AcceptRow> for AcceptChange {
    fn from((signature_hash, bid_signature_hash, signer, signed_at, seq): AcceptRow) -> Self {
        Self {
            kind: "accept",
            signature_hash,
            bid_signature_hash,
            signer,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FedList<T> {
    pub data: Vec<T>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FedBidEntry {
    pub signature_hash: String,
    pub item_id: String,
    pub signer: String,
    pub price: String,
    pub expires_at: i64,
    pub fingerprint: String,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<BidRow> for FedBidEntry {
    fn from(
        (signature_hash, item_id, signer, price, expires_at, fingerprint, signed_at, seq): BidRow,
    ) -> Self {
        Self {
            signature_hash,
            item_id,
            signer,
            price,
            expires_at,
            fingerprint,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FedOrderEntry {
    pub signature_hash: String,
    pub item_id: String,
    pub signer: String,
    pub price: String,
    pub expires_at: i64,
    pub signed_at: i64,
    pub seq: i64,
}

impl From<OrderRow> for FedOrderEntry {
    fn from(
        (signature_hash, item_id, signer, price, expires_at, signed_at, seq): OrderRow,
    ) -> Self {
        Self {
            signature_hash,
            item_id,
            signer,
            price,
            expires_at,
            signed_at,
            seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FedTradeEntry {
    pub signature_hash: String,
    pub order_signature_hash: String,
    pub buyer: String,
    pub tx_hash: String,
    pub taken_at: i64,
    pub signed_at: i64,
    pub seq: i64,
}

impl FedTradeEntry {
    fn from_row(
        (signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq): TradeRow,
    ) -> Self {
        Self {
            signature_hash,
            order_signature_hash,
            buyer,
            tx_hash,
            taken_at,
            signed_at,
            seq,
        }
    }
}

fn err_json(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<FedWriteBody>) {
    (
        code,
        Json(FedWriteBody::Error(FedErrorBody {
            ok: false,
            message: message.into(),
        })),
    )
}

fn ok_json(sig_hash: String) -> (StatusCode, Json<FedWriteBody>) {
    (
        StatusCode::OK,
        Json(FedWriteBody::Ack(FedAck {
            ok: true,
            signature_hash: sig_hash,
        })),
    )
}

fn parse_signed<T: TypedMessage + DeserializeOwned>(
    body: &[u8],
) -> Result<Signed<T>, (StatusCode, Json<FedWriteBody>)> {
    serde_json::from_slice::<Signed<T>>(body).map_err(|e| {
        err_json(
            StatusCode::BAD_REQUEST,
            format!("invalid Signed<{}>: {}", T::PRIMARY_TYPE, e),
        )
    })
}

async fn preflight<T: TypedMessage + DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<(Signed<T>, String), (StatusCode, Json<FedWriteBody>)> {
    let outer_signer = require_signer(headers, method, path)
        .map_err(|e| err_json(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e)))?;

    let signed: Signed<T> = parse_signed(body)?;

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = signed.verify(&outer_signer, now) {
        return Err(err_json(
            StatusCode::UNAUTHORIZED,
            format!("signature verify: {}", e),
        ));
    }

    if !signed.domain.name.eq_ignore_ascii_case(&state.domain.name) {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            format!("domain mismatch: expected {}", state.domain.name),
        ));
    }

    if let Err(e) = state
        .replay
        .check_and_record(&outer_signer, &signed.nonce, signed.signed_at)
        .await
    {
        return Err(match e {
            FedError::DuplicateNonce { .. } => err_json(StatusCode::CONFLICT, e.to_string()),
            _ => err_json(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        });
    }

    if matches!(state.limiter.check(&outer_signer), RateLimitDecision::Deny) {
        return Err(err_json(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded",
        ));
    }

    Ok((signed, outer_signer))
}

fn map_apply_err(e: ApiError) -> (StatusCode, Json<FedWriteBody>) {
    let (code, message) = match e {
        ApiError::Http(catalyrst_types::HttpError { code, message }) => (code, message),
        ApiError::Database(de) => {
            tracing::error!(error = %de, "federation apply database error");
            (500, "database error".to_string())
        }
        other => (500, other.to_string()),
    };
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    err_json(status, message)
}

pub async fn place_bid(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) =
        match preflight::<BidPlace>(&state, &headers, "post", "/v1/federation/bid", &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    match apply::apply_bid_place(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn cancel_bid(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) =
        match preflight::<BidCancel>(&state, &headers, "post", "/v1/federation/bid/cancel", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };

    match lookup_bid_signer(&state.pool, &signed.message.bid_signature_hash).await {
        Ok(Some(original)) => {
            if !original.eq_ignore_ascii_case(&signer) {
                return err_json(
                    StatusCode::FORBIDDEN,
                    "only the bid signer may cancel this bid",
                );
            }
        }
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "bid not found"),
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_bid_cancel(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn accept_bid(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) =
        match preflight::<BidAccept>(&state, &headers, "post", "/v1/federation/bid/accept", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };

    let item_id = match lookup_bid_item_id(&state.pool, &signed.message.bid_signature_hash).await {
        Ok(Some(i)) => i,
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "bid not found"),
        Err(e) => return map_apply_err(e),
    };

    let owns = match signer_owns_any_nft_for_item(&state.pool, &signer, &item_id).await {
        Ok(b) => b,
        Err(e) => return map_apply_err(e),
    };
    if !owns {
        return err_json(
            StatusCode::FORBIDDEN,
            "signer does not own any NFT for this bid's item_id",
        );
    }

    match apply::apply_bid_accept(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) =
        match preflight::<OrderCreate>(&state, &headers, "post", "/v1/federation/order", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };

    let item_id = &signed.message.item_id;

    match signer_has_active_lease_for_item(&state.pool, &signer, item_id).await {
        Ok(true) => {
            return err_json(
                StatusCode::FORBIDDEN,
                "item is in the return window (leased); cannot be listed until it unlocks",
            )
        }
        Ok(false) => {}
        Err(e) => return map_apply_err(e),
    }

    match signer_owns_any_nft_for_item(&state.pool, &signer, item_id).await {
        Ok(true) => {}
        Ok(false) => {
            return err_json(
                StatusCode::FORBIDDEN,
                "signer does not own any NFT for this order's item_id",
            )
        }
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_order_create(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn cancel_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) = match preflight::<OrderCancel>(
        &state,
        &headers,
        "post",
        "/v1/federation/order/cancel",
        &body,
    )
    .await
    {
        Ok(x) => x,
        Err(e) => return e,
    };

    match lookup_order_signer(&state.pool, &signed.message.order_signature_hash).await {
        Ok(Some(original)) => {
            if !original.eq_ignore_ascii_case(&signer) {
                return err_json(
                    StatusCode::FORBIDDEN,
                    "only the order signer may cancel this order",
                );
            }
        }
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "order not found"),
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_order_cancel(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn record_trade(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<FedWriteBody>) {
    let (signed, signer) =
        match preflight::<TradeRecord>(&state, &headers, "post", "/v1/federation/trade", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };

    match order_exists(&state.pool, &signed.message.order_signature_hash).await {
        Ok(true) => {}
        Ok(false) => return err_json(StatusCode::NOT_FOUND, "order not found"),
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_trade_record(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn snapshot(State(state): State<AppState>) -> Result<Json<MarketSnapshot>, ApiError> {
    let (bids_max, orders_max, trades_max, cancels_max, accepts_max) = tokio::try_join!(
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MAX(seq) FROM market_bids_local")
            .fetch_one(&state.pool),
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MAX(seq) FROM market_orders_local")
            .fetch_one(&state.pool),
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MAX(seq) FROM market_trades_local")
            .fetch_one(&state.pool),
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MAX(seq) FROM market_cancellations")
            .fetch_one(&state.pool),
        sqlx::query_as::<_, (Option<i64>,)>("SELECT MAX(seq) FROM market_bid_acceptances")
            .fetch_one(&state.pool),
    )?;

    let bid_hashes: Vec<(String,)> =
        sqlx::query_as("SELECT signature_hash FROM market_bids_local ORDER BY signature_hash ASC")
            .fetch_all(&state.pool)
            .await?;
    let order_hashes: Vec<(String,)> = sqlx::query_as(
        "SELECT signature_hash FROM market_orders_local ORDER BY signature_hash ASC",
    )
    .fetch_all(&state.pool)
    .await?;
    let trade_hashes: Vec<(String,)> = sqlx::query_as(
        "SELECT signature_hash FROM market_trades_local ORDER BY signature_hash ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut h = Sha256::new();
    for (s,) in bid_hashes
        .iter()
        .chain(order_hashes.iter())
        .chain(trade_hashes.iter())
    {
        h.update(s.as_bytes());
    }
    let log_hash = hex::encode(h.finalize());

    Ok(Json(MarketSnapshot {
        latest_bids_seq: bids_max.0.unwrap_or(0),
        latest_orders_seq: orders_max.0.unwrap_or(0),
        latest_trades_seq: trades_max.0.unwrap_or(0),
        latest_cancellations_seq: cancels_max.0.unwrap_or(0),
        latest_acceptances_seq: accepts_max.0.unwrap_or(0),
        log_hash,
        domain: "DecentralandMarket",
    }))
}

pub async fn changes(
    State(state): State<AppState>,
    Query(q): Query<ChangesQuery>,
) -> Result<Json<MarketChanges>, ApiError> {
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);

    let (bids, orders, trades, cancels, accepts) = tokio::try_join!(
        sqlx::query_as::<_, BidRow>(
            "SELECT signature_hash, item_id, signer, price::text, expires_at, fingerprint, signed_at, seq \
               FROM market_bids_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, OrderRow>(
            "SELECT signature_hash, item_id, signer, price::text, expires_at, signed_at, seq \
               FROM market_orders_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, TradeRow>(
            "SELECT signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq \
               FROM market_trades_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, CancelRow>(
            "SELECT signature_hash, target_signature_hash, kind, signer, signed_at, seq \
               FROM market_cancellations WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, AcceptRow>(
            "SELECT signature_hash, bid_signature_hash, signer, signed_at, seq \
               FROM market_bid_acceptances WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
    )?;

    Ok(Json(MarketChanges {
        bids: bids.into_iter().map(BidChange::from).collect(),
        orders: orders.into_iter().map(OrderChange::from).collect(),
        trades: trades.into_iter().map(TradeChange::from_row).collect(),
        cancellations: cancels.into_iter().map(CancelChange::from).collect(),
        acceptances: accepts.into_iter().map(AcceptChange::from).collect(),
    }))
}

pub async fn list_bids(
    State(state): State<AppState>,
) -> Result<Json<FedList<FedBidEntry>>, ApiError> {
    let rows: Vec<BidRow> = sqlx::query_as(
        "SELECT signature_hash, item_id, signer, price::text, expires_at, fingerprint, signed_at, seq \
           FROM market_bids_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<FedBidEntry> = rows.into_iter().map(FedBidEntry::from).collect();
    let total = data.len();
    Ok(Json(FedList { data, total }))
}

pub async fn list_orders(
    State(state): State<AppState>,
) -> Result<Json<FedList<FedOrderEntry>>, ApiError> {
    let rows: Vec<OrderRow> = sqlx::query_as(
        "SELECT signature_hash, item_id, signer, price::text, expires_at, signed_at, seq \
           FROM market_orders_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<FedOrderEntry> = rows.into_iter().map(FedOrderEntry::from).collect();
    let total = data.len();
    Ok(Json(FedList { data, total }))
}

pub async fn list_trades(
    State(state): State<AppState>,
) -> Result<Json<FedList<FedTradeEntry>>, ApiError> {
    let rows: Vec<TradeRow> = sqlx::query_as(
        "SELECT signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq \
           FROM market_trades_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<FedTradeEntry> = rows.into_iter().map(FedTradeEntry::from_row).collect();
    let total = data.len();
    Ok(Json(FedList { data, total }))
}

#[cfg(test)]
mod tests {
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
            log_hash: "4230d9f8f415e7d5b15060a635b0602060bbdf1796195b8f6f277a7dbe092fae"
                .to_string(),
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
}
