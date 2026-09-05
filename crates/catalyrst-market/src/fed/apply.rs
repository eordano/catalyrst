use catalyrst_fed::Signed;
use serde_json::json;
use sqlx::PgPool;

use crate::fed::ids::signature_hash_hex;
use crate::fed::messages::{BidAccept, BidCancel, BidPlace, OrderCancel, OrderCreate, TradeRecord};
use crate::http::response::ApiError;

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

pub struct AppliedAction {
    pub signature_hash: String,
}

pub async fn apply_bid_place(
    pool: &PgPool,
    signed: &Signed<BidPlace>,
    signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_bids_local \
            (signature_hash, item_id, signer, price, expires_at, fingerprint, signed_at, message_payload, received_at) \
         VALUES ($1, $2, $3, $4::numeric, $5, $6, $7, $8, $9) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.item_id)
    .bind(signer.to_ascii_lowercase())
    .bind(&signed.message.price)
    .bind(signed.message.expires_at)
    .bind(&signed.message.fingerprint)
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}

pub async fn apply_bid_cancel(
    pool: &PgPool,
    signed: &Signed<BidCancel>,
    signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_cancellations \
            (signature_hash, target_signature_hash, kind, signer, signed_at, message_payload, received_at) \
         VALUES ($1, $2, 'bid', $3, $4, $5, $6) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.bid_signature_hash)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}

pub async fn apply_bid_accept(
    pool: &PgPool,
    signed: &Signed<BidAccept>,
    signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_bid_acceptances \
            (signature_hash, bid_signature_hash, signer, signed_at, message_payload, received_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.bid_signature_hash)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}

pub async fn apply_order_create(
    pool: &PgPool,
    signed: &Signed<OrderCreate>,
    signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_orders_local \
            (signature_hash, item_id, signer, price, expires_at, signed_at, message_payload, received_at) \
         VALUES ($1, $2, $3, $4::numeric, $5, $6, $7, $8) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.item_id)
    .bind(signer.to_ascii_lowercase())
    .bind(&signed.message.price)
    .bind(signed.message.expires_at)
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}

pub async fn apply_order_cancel(
    pool: &PgPool,
    signed: &Signed<OrderCancel>,
    signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_cancellations \
            (signature_hash, target_signature_hash, kind, signer, signed_at, message_payload, received_at) \
         VALUES ($1, $2, 'order', $3, $4, $5, $6) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.order_signature_hash)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}

pub async fn apply_trade_record(
    pool: &PgPool,
    signed: &Signed<TradeRecord>,
    _signer: &str,
) -> Result<AppliedAction, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    sqlx::query(
        "INSERT INTO market_trades_local \
            (signature_hash, order_signature_hash, buyer, tx_hash, taken_at, signed_at, message_payload, received_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.order_signature_hash)
    .bind(signed.message.buyer.to_ascii_lowercase())
    .bind(&signed.message.tx_hash)
    .bind(signed.message.taken_at)
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AppliedAction {
        signature_hash: sig_hash,
    })
}
