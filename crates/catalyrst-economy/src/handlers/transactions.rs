use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::ports::transaction::{
    parse_send_transaction_request, reservation_disposition, ReservationDisposition, TransactionRow,
};
use crate::AppState;

pub async fn send_transaction(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;
    let tx = parse_send_transaction_request(&body)?;

    state
        .transaction
        .check_data(&state.config, &state.contracts, &tx)
        .await?;

    let user_address = tx.from.to_lowercase();
    let session_id = uuid::Uuid::new_v4().to_string();

    state
        .transaction
        .reserve_quota(
            state.config.max_transactions_per_day,
            &user_address,
            &session_id,
        )
        .await?;

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
                if let Err(release_err) = state.transaction.release_reservation(&session_id).await {
                    tracing::error!(
                        session_id = %session_id,
                        user_address = %user_address,
                        error = %release_err,
                        "failed to release reservation after a pre-broadcast failure"
                    );
                }
            }
            return Err(err);
        }
    };

    state
        .transaction
        .confirm_reservation(&session_id, &tx_hash)
        .await?;

    Ok(Json(json!({ "ok": true, "txHash": tx_hash })))
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
