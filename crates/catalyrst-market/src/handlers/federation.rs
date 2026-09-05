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

type HashRows = Vec<(String,)>;

type BidRow = (String, String, String, String, i64, String, i64, i64);
type OrderRow = (String, String, String, String, i64, i64, i64);
type TradeRow = (String, String, String, String, i64, i64, i64);
type CancelRow = (String, String, String, String, i64, i64);
type AcceptRow = (String, String, String, i64, i64);

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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
) -> Result<(Signed<T>, catalyrst_crypto::Signer), (StatusCode, Json<FedWriteBody>)> {
    let outer_signer = require_signer(headers, method, path)
        .await
        .map_err(|e| err_json(StatusCode::UNAUTHORIZED, format!("auth chain: {}", e)))?;

    let signed: Signed<T> = parse_signed(body)?;

    let now = chrono::Utc::now().timestamp();
    if let Err(e) = signed.verify(outer_signer.as_str(), now) {
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
        .check_and_record(outer_signer.as_str(), &signed.nonce, signed.signed_at)
        .await
    {
        return Err(match e {
            FedError::DuplicateNonce { .. } => err_json(StatusCode::CONFLICT, e.to_string()),
            _ => err_json(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        });
    }

    if matches!(
        state.limiter.check(outer_signer.as_str()),
        RateLimitDecision::Deny
    ) {
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

#[utoipa::path(
    post,
    path = "/v1/federation/bid",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody)
    )
)]
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
    match apply::apply_bid_place(&state.pool, &signed, signer.as_str()).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/federation/bid/cancel",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody)
    )
)]
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
            if !original.eq_ignore_ascii_case(signer.as_str()) {
                return err_json(
                    StatusCode::FORBIDDEN,
                    "only the bid signer may cancel this bid",
                );
            }
        }
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "bid not found"),
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_bid_cancel(&state.pool, &signed, signer.as_str()).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/federation/bid/accept",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody)
    )
)]
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

    let owns = match signer_owns_any_nft_for_item(&state.pool, signer.as_str(), &item_id).await {
        Ok(b) => b,
        Err(e) => return map_apply_err(e),
    };
    if !owns {
        return err_json(
            StatusCode::FORBIDDEN,
            "signer does not own any NFT for this bid's item_id",
        );
    }

    match apply::apply_bid_accept(&state.pool, &signed, signer.as_str()).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/federation/order",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody)
    )
)]
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

    let (lease_res, owns_res) = tokio::join!(
        signer_has_active_lease_for_item(&state.pool, signer.as_str(), item_id),
        signer_owns_any_nft_for_item(&state.pool, signer.as_str(), item_id),
    );

    let leased = match lease_res {
        Ok(b) => b,
        Err(e) => return map_apply_err(e),
    };
    if leased {
        return err_json(
            StatusCode::FORBIDDEN,
            "item is in the return window (leased); cannot be listed until it unlocks",
        );
    }
    match owns_res {
        Ok(true) => {}
        Ok(false) => {
            return err_json(
                StatusCode::FORBIDDEN,
                "signer does not own any NFT for this order's item_id",
            )
        }
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_order_create(&state.pool, &signed, signer.as_str()).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/federation/order/cancel",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 403, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody)
    )
)]
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
            if !original.eq_ignore_ascii_case(signer.as_str()) {
                return err_json(
                    StatusCode::FORBIDDEN,
                    "only the order signer may cancel this order",
                );
            }
        }
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "order not found"),
        Err(e) => return map_apply_err(e),
    }

    match apply::apply_order_cancel(&state.pool, &signed, signer.as_str()).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

#[utoipa::path(
    post,
    path = "/v1/federation/trade",
    tag = "market-federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = FedAck),
        (status = 400, body = crate::http::response::MarketErrorBody),
        (status = 401, body = crate::http::response::MarketErrorBody),
        (status = 404, body = crate::http::response::MarketErrorBody)
    )
)]
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

    match apply::apply_trade_record(&state.pool, &signed, signer.as_str()).await {
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

#[utoipa::path(
    get,
    path = "/federation/market/snapshot",
    tag = "market-federation",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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

    let (bid_hashes, order_hashes, trade_hashes): (HashRows, HashRows, HashRows) = tokio::try_join!(
        sqlx::query_as("SELECT signature_hash FROM market_bids_local ORDER BY signature_hash ASC")
            .fetch_all(&state.pool),
        sqlx::query_as(
            "SELECT signature_hash FROM market_orders_local ORDER BY signature_hash ASC",
        )
        .fetch_all(&state.pool),
        sqlx::query_as(
            "SELECT signature_hash FROM market_trades_local ORDER BY signature_hash ASC",
        )
        .fetch_all(&state.pool),
    )?;

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

#[utoipa::path(
    get,
    path = "/federation/market/changes",
    tag = "market-federation",
    params(("since" = Option<i64>, Query), ("limit" = Option<i64>, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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

#[utoipa::path(
    get,
    path = "/v1/federation/bids",
    tag = "market-federation",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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

#[utoipa::path(
    get,
    path = "/v1/federation/orders",
    tag = "market-federation",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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

#[utoipa::path(
    get,
    path = "/v1/federation/trades",
    tag = "market-federation",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 500, body = crate::http::response::MarketErrorBody)
    )
)]
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
#[path = "federation_tests.rs"]
mod tests;
