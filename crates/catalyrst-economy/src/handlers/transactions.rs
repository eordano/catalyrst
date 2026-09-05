use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::http::errors::ApiError;
use crate::ports::transaction::{
    parse_send_transaction_request, reservation_disposition, MetaTxSender, ReservationDisposition,
    TransactionRow,
};
use crate::ports::upstream::UpstreamForwarder;
use crate::AppState;

pub async fn send_transaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let body = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;
    let tx = parse_send_transaction_request(&body)?;

    let sender = state
        .transaction
        .check_data(&state.config, &state.contracts, &tx)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();

    state
        .transaction
        .reserve_quota(state.config.max_transactions_per_day, &sender, &session_id)
        .await?;

    if let Some(upstream) = state.transaction.upstream_route() {
        return forward_upstream(&state, upstream, &headers, &body, &sender, &session_id).await;
    }

    let tx_hash = match state
        .transaction
        .send_meta_transaction(&state.config, &tx)
        .await
    {
        Ok(hash) => hash,
        Err(err) => {
            if matches!(
                reservation_disposition(&err),
                ReservationDisposition::Release
            ) {
                release_quota_slot(&state, &session_id, &sender).await;
            }
            return Err(err);
        }
    };

    state
        .transaction
        .confirm_reservation(&session_id, &tx_hash)
        .await?;

    Ok(Json(json!({ "ok": true, "txHash": tx_hash })).into_response())
}

/// Relays the validated body to the upstream transactions server and passes its
/// response (status + body) through verbatim: the client sees exactly what it
/// would see talking to the upstream directly.
async fn forward_upstream(
    state: &AppState,
    upstream: &UpstreamForwarder,
    headers: &HeaderMap,
    body: &[u8],
    sender: &MetaTxSender,
    session_id: &str,
) -> Result<Response, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());

    let forwarded = match upstream.forward(body, content_type).await {
        Ok(f) => f,
        Err(err) => {
            if matches!(
                reservation_disposition(&err),
                ReservationDisposition::Release
            ) {
                release_quota_slot(state, session_id, sender).await;
            }
            return Err(err);
        }
    };

    if forwarded.is_success() {
        match forwarded.tx_hash() {
            // The upstream already broadcast: a local bookkeeping failure must
            // not hide the upstream response from the client.
            Some(hash) => {
                if let Err(e) = state
                    .transaction
                    .confirm_reservation(session_id, &hash)
                    .await
                {
                    tracing::error!(
                        session_id = %session_id,
                        user_address = %sender.as_str(),
                        error = %e,
                        "failed to confirm reservation after an upstream broadcast"
                    );
                }
            }
            None => tracing::warn!(
                session_id = %session_id,
                user_address = %sender.as_str(),
                "upstream 2xx response carries no txHash; the quota slot stays consumed"
            ),
        }
    } else if refunds_quota_slot(forwarded.status()) {
        release_quota_slot(state, session_id, sender).await;
    }

    Ok(forwarded.into_response())
}

/// Whether a non-2xx upstream status proves the transaction was never
/// broadcast, so the quota slot can be refunded. A 4xx is a validation or
/// rate-limit rejection issued before any broadcast. Every 5xx is
/// indeterminate: the upstream sits behind Cloudflare, so 502/504 gateway
/// failures and Cloudflare's own 522/524 arrive after the request reached the
/// intermediary, and even a 500/503 can be emitted by the origin after it
/// already submitted the transaction -- none of them proves the broadcast
/// never happened. A kept slot merely ages out of the 24h quota window; a
/// false refund breaks the daily-quota invariant, so every 5xx keeps the
/// slot.
fn refunds_quota_slot(status: u16) -> bool {
    (400..500).contains(&status)
}

async fn release_quota_slot(state: &AppState, session_id: &str, sender: &MetaTxSender) {
    if let Err(release_err) = state.transaction.release_reservation(session_id).await {
        tracing::error!(
            session_id = %session_id,
            user_address = %sender.as_str(),
            error = %release_err,
            "failed to release reservation after a pre-broadcast failure"
        );
    }
}

pub async fn get_user_transactions(
    State(state): State<AppState>,
    Path(user_address): Path<String>,
) -> Result<Json<Vec<TransactionRow>>, ApiError> {
    let rows = state
        .transaction
        .get_by_user_address(&user_address.to_lowercase())
        .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::refunds_quota_slot;

    #[test]
    fn only_4xx_upstream_statuses_refund_the_quota_slot() {
        for status in [400u16, 404, 422, 429, 499] {
            assert!(
                refunds_quota_slot(status),
                "{status} is a pre-broadcast rejection and refunds the slot"
            );
        }
        for status in [500u16, 501, 502, 503, 504, 520, 522, 524] {
            assert!(
                !refunds_quota_slot(status),
                "{status} is broadcast-indeterminate and keeps the slot"
            );
        }
    }
}
