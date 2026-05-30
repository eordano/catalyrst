use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::ports::transaction::{parse_send_transaction_request, TransactionRow};
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

    let tx_hash = state
        .transaction
        .send_meta_transaction(&state.config, &tx)
        .await?;

    state
        .transaction
        .insert(&tx_hash, &tx.from.to_lowercase())
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
