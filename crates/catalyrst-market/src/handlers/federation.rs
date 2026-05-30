use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use catalyrst_fed::{FedError, RateLimitDecision, Signed, TypedMessage};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::auth_chain::require_signer;
use crate::fed::apply;
use crate::fed::authority::{
    lookup_bid_item_id, lookup_bid_signer, lookup_order_signer, order_exists,
    signer_owns_any_nft_for_item,
};
use crate::fed::messages::{
    BidAccept, BidCancel, BidPlace, OrderCancel, OrderCreate, TradeRecord,
};
use crate::http::response::ApiError;
use crate::AppState;

fn err_json(code: StatusCode, message: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    let m = message.into();
    (code, Json(json!({ "ok": false, "message": m })))
}

fn ok_json(sig_hash: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "signature_hash": sig_hash })),
    )
}

fn parse_signed<T: TypedMessage + DeserializeOwned>(
    body: &[u8],
) -> Result<Signed<T>, (StatusCode, Json<serde_json::Value>)> {
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
) -> Result<(Signed<T>, String), (StatusCode, Json<serde_json::Value>)> {
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
        return Err(err_json(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"));
    }

    Ok((signed, outer_signer))
}

fn map_apply_err(e: ApiError) -> (StatusCode, Json<serde_json::Value>) {
    let (code, message) = match e {
        ApiError::Http(catalyrst_types::HttpError { code, message }) => (code, message),
        ApiError::Database(de) => {
            tracing::error!(error = %de, "federation apply database error");
            (500, "database error".to_string())
        }
        other => (500, other.to_string()),
    };
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(json!({ "ok": false, "message": message })))
}

pub async fn place_bid(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
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
) -> (StatusCode, Json<serde_json::Value>) {
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
) -> (StatusCode, Json<serde_json::Value>) {
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
) -> (StatusCode, Json<serde_json::Value>) {
    let (signed, signer) =
        match preflight::<OrderCreate>(&state, &headers, "post", "/v1/federation/order", &body)
            .await
        {
            Ok(x) => x,
            Err(e) => return e,
        };
    match apply::apply_order_create(&state.pool, &signed, &signer).await {
        Ok(out) => ok_json(out.signature_hash),
        Err(e) => map_apply_err(e),
    }
}

pub async fn cancel_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let (signed, signer) =
        match preflight::<OrderCancel>(
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
) -> (StatusCode, Json<serde_json::Value>) {
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

pub async fn snapshot(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
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

    let bid_hashes: Vec<(String,)> = sqlx::query_as(
        "SELECT signature_hash FROM market_bids_local ORDER BY signature_hash ASC",
    )
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
    for (s,) in bid_hashes.iter().chain(order_hashes.iter()).chain(trade_hashes.iter()) {
        h.update(s.as_bytes());
    }
    let log_hash = hex::encode(h.finalize());

    Ok(Json(json!({
        "latest_bids_seq":         bids_max.0.unwrap_or(0),
        "latest_orders_seq":       orders_max.0.unwrap_or(0),
        "latest_trades_seq":       trades_max.0.unwrap_or(0),
        "latest_cancellations_seq": cancels_max.0.unwrap_or(0),
        "latest_acceptances_seq":  accepts_max.0.unwrap_or(0),
        "log_hash":                log_hash,
        "domain":                  "DecentralandMarket",
    })))
}

pub async fn changes(
    State(state): State<AppState>,
    Query(q): Query<ChangesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);

    let (bids, orders, trades, cancels, accepts) = tokio::try_join!(
        sqlx::query_as::<_, (String, String, String, String, i64, String, i64, i64)>(
            "SELECT signature_hash, item_id, signer, price::text, expires_at, fingerprint, signed_at, seq \
               FROM market_bids_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, (String, String, String, String, i64, i64, i64)>(
            "SELECT signature_hash, item_id, signer, price::text, expires_at, signed_at, seq \
               FROM market_orders_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, (String, String, String, String, i64, i64, i64)>(
            "SELECT signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq \
               FROM market_trades_local WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, (String, String, String, String, i64, i64)>(
            "SELECT signature_hash, target_signature_hash, kind, signer, signed_at, seq \
               FROM market_cancellations WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
        sqlx::query_as::<_, (String, String, String, i64, i64)>(
            "SELECT signature_hash, bid_signature_hash, signer, signed_at, seq \
               FROM market_bid_acceptances WHERE seq > $1 ORDER BY seq ASC LIMIT $2",
        )
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.pool),
    )?;

    let bids_json: Vec<serde_json::Value> = bids
        .into_iter()
        .map(|(sig, item, signer, price, expires_at, fp, signed_at, seq)| {
            json!({
                "kind": "bid",
                "signature_hash": sig,
                "item_id": item,
                "signer": signer,
                "price": price,
                "expires_at": expires_at,
                "fingerprint": fp,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    let orders_json: Vec<serde_json::Value> = orders
        .into_iter()
        .map(|(sig, item, signer, price, expires_at, signed_at, seq)| {
            json!({
                "kind": "order",
                "signature_hash": sig,
                "item_id": item,
                "signer": signer,
                "price": price,
                "expires_at": expires_at,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    let trades_json: Vec<serde_json::Value> = trades
        .into_iter()
        .map(|(sig, order_sig, buyer, tx, taken_at, signed_at, seq)| {
            json!({
                "kind": "trade",
                "signature_hash": sig,
                "order_signature_hash": order_sig,
                "buyer": buyer,
                "tx_hash": tx,
                "taken_at": taken_at,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    let cancels_json: Vec<serde_json::Value> = cancels
        .into_iter()
        .map(|(sig, target, kind, signer, signed_at, seq)| {
            json!({
                "kind": "cancel",
                "signature_hash": sig,
                "target_signature_hash": target,
                "target_kind": kind,
                "signer": signer,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    let accepts_json: Vec<serde_json::Value> = accepts
        .into_iter()
        .map(|(sig, bid_sig, signer, signed_at, seq)| {
            json!({
                "kind": "accept",
                "signature_hash": sig,
                "bid_signature_hash": bid_sig,
                "signer": signer,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();

    Ok(Json(json!({
        "bids": bids_json,
        "orders": orders_json,
        "trades": trades_json,
        "cancellations": cancels_json,
        "acceptances": accepts_json,
    })))
}

pub async fn list_bids(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows: Vec<(String, String, String, String, i64, String, i64, i64)> = sqlx::query_as(
        "SELECT signature_hash, item_id, signer, price::text, expires_at, fingerprint, signed_at, seq \
           FROM market_bids_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(sig, item, signer, price, expires_at, fp, signed_at, seq)| {
            json!({
                "signature_hash": sig,
                "item_id": item,
                "signer": signer,
                "price": price,
                "expires_at": expires_at,
                "fingerprint": fp,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data, "total": data.len() })))
}

pub async fn list_orders(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows: Vec<(String, String, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT signature_hash, item_id, signer, price::text, expires_at, signed_at, seq \
           FROM market_orders_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(sig, item, signer, price, expires_at, signed_at, seq)| {
            json!({
                "signature_hash": sig,
                "item_id": item,
                "signer": signer,
                "price": price,
                "expires_at": expires_at,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data, "total": data.len() })))
}

pub async fn list_trades(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows: Vec<(String, String, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, seq \
           FROM market_trades_local ORDER BY seq DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(sig, order_sig, buyer, tx, taken_at, signed_at, seq)| {
            json!({
                "signature_hash": sig,
                "order_signature_hash": order_sig,
                "buyer": buyer,
                "tx_hash": tx,
                "taken_at": taken_at,
                "signed_at": signed_at,
                "seq": seq,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data, "total": data.len() })))
}
