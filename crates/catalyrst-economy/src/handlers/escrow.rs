use alloy::primitives::Address;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::require_admin;
use crate::handlers::{idempotency_key, require_broadcast_enabled};
use crate::http::errors::ApiError;
use crate::ports::broker::{parse_address, parse_token_id, BrokerCall};
use crate::ports::escrow::{build_reclaim, build_release};
use crate::AppState;

#[derive(Debug, Clone, Copy)]
enum EscrowAction {
    Reclaim,
    Release,
}

impl EscrowAction {
    fn as_str(self) -> &'static str {
        match self {
            EscrowAction::Reclaim => "reclaim",
            EscrowAction::Release => "release",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimBody {
    pub collection: String,

    pub token_id: String,

    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseBody {
    pub collection: String,

    pub token_id: String,

    pub buyer: String,

    #[serde(default)]
    pub idempotency_key: Option<String>,
}

pub async fn reclaim(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ReclaimBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let collection = parse_address("collection", &body.collection)?;
    let token_id = parse_token_id(&body.token_id)?;
    let escrow = resolve_escrow(&state)?;
    let signer = resolve_signer(&state)?;
    let idem = idempotency_key(&headers, body.idempotency_key.as_deref());

    let call = build_reclaim(escrow, collection, token_id);
    let tx_hash = broadcast_action(
        &state,
        signer,
        EscrowAction::Reclaim,
        call,
        escrow,
        &collection_hex(collection),
        &body.token_id,
        None,
        idem.as_deref(),
    )
    .await?;

    Ok(Json(json!({ "ok": true, "txHash": tx_hash })))
}

pub async fn release(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ReleaseBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let collection = parse_address("collection", &body.collection)?;
    let token_id = parse_token_id(&body.token_id)?;
    let buyer = parse_address("buyer", &body.buyer)?;
    let escrow = resolve_escrow(&state)?;
    let signer = resolve_signer(&state)?;
    let idem = idempotency_key(&headers, body.idempotency_key.as_deref());

    let call = build_release(escrow, collection, token_id, buyer);
    let tx_hash = broadcast_action(
        &state,
        signer,
        EscrowAction::Release,
        call,
        escrow,
        &collection_hex(collection),
        &body.token_id,
        Some(&format!("{buyer:#x}")),
        idem.as_deref(),
    )
    .await?;

    Ok(Json(json!({ "ok": true, "txHash": tx_hash })))
}

fn collection_hex(collection: Address) -> String {
    format!("{collection:#x}")
}

fn resolve_escrow(state: &AppState) -> Result<Address, ApiError> {
    let raw = state
        .config
        .landiler_escrow_address
        .as_deref()
        .ok_or_else(|| {
            ApiError::RelayerUnavailable(
                "LANDILER_ESCROW_ADDRESS is not configured; escrow reclaim/release is unavailable."
                    .into(),
            )
        })?;
    parse_address("LANDILER_ESCROW_ADDRESS", raw).map_err(|_| {
        ApiError::Internal("LANDILER_ESCROW_ADDRESS is set but is not a valid address".into())
    })
}

fn resolve_signer(state: &AppState) -> Result<&crate::ports::signer::DirectSigner, ApiError> {
    require_broadcast_enabled(state, "escrow actions")?;
    state.transaction.direct_signer().ok_or_else(|| {
        ApiError::RelayerUnavailable(
            "Escrow actions require the direct JSON-RPC signer (set META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + RPC_URL).".into(),
        )
    })
}

#[allow(clippy::too_many_arguments)]
async fn broadcast_action(
    state: &AppState,
    signer: &crate::ports::signer::DirectSigner,
    action: EscrowAction,
    call: BrokerCall,
    escrow: Address,
    collection_hex: &str,
    token_id_text: &str,
    buyer_hex: Option<&str>,
    idem: Option<&str>,
) -> Result<String, ApiError> {
    let escrow_hex = format!("{escrow:#x}");
    let chain_id = state.config.collections_chain_id as i64;

    let tx_hash = match idem {
        Some(key) => {
            let claim = sqlx::query(
                "INSERT INTO escrow_actions \
                 (idempotency_key, action, collection, token_id, buyer, escrow_address, chain_id, status) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending') \
                 ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING",
            )
            .bind(key)
            .bind(action.as_str())
            .bind(collection_hex)
            .bind(token_id_text)
            .bind(buyer_hex)
            .bind(&escrow_hex)
            .bind(chain_id)
            .execute(&state.pool)
            .await?;

            if claim.rows_affected() == 0 {
                let (existing_tx, status): (Option<String>, String) = sqlx::query_as(
                    "SELECT tx_hash, status FROM escrow_actions WHERE idempotency_key = $1",
                )
                .bind(key)
                .fetch_one(&state.pool)
                .await?;
                match status.as_str() {
                    "sent" => {
                        let tx_hash = existing_tx.ok_or_else(|| {
                            ApiError::Internal(format!(
                                "escrow action for idempotencyKey {key:?} is 'sent' but has no recorded txHash"
                            ))
                        })?;
                        tracing::info!(idempotency_key = %key, tx_hash = %tx_hash, action = action.as_str(), "escrow action idempotent replay → returning recorded txHash");
                        return Ok(tx_hash);
                    }

                    "error" => {
                        let rearmed = sqlx::query(
                            "UPDATE escrow_actions SET status = 'pending', updated_at = NOW() WHERE idempotency_key = $1 AND status = 'error'",
                        )
                        .bind(key)
                        .execute(&state.pool)
                        .await?;
                        if rearmed.rows_affected() == 0 {
                            return Err(ApiError::Conflict(format!(
                                "an escrow {} for idempotencyKey {key:?} is already in flight; not re-broadcasting — retry after it settles",
                                action.as_str()
                            )));
                        }
                        tracing::info!(idempotency_key = %key, action = action.as_str(), "re-arming errored escrow action for re-broadcast (prior attempt failed pre-broadcast; no NFT moved)");
                    }

                    _ => {
                        return Err(ApiError::Conflict(format!(
                            "an escrow {} for idempotencyKey {key:?} is in flight (status {status:?}); not re-broadcasting — poll the recorded action or reconcile rather than retrying",
                            action.as_str()
                        )));
                    }
                }
            }

            match signer.send_direct_call(call.to, call.data).await {
                Ok(tx_hash) => {
                    sqlx::query(
                        "UPDATE escrow_actions SET tx_hash = $2, status = 'sent', updated_at = NOW() WHERE idempotency_key = $1",
                    )
                    .bind(key)
                    .bind(&tx_hash)
                    .execute(&state.pool)
                    .await?;
                    tx_hash
                }
                Err(e) => {
                    if let Err(db) = sqlx::query(
                        "UPDATE escrow_actions SET status = 'error', updated_at = NOW() WHERE idempotency_key = $1 AND status = 'pending'",
                    )
                    .bind(key)
                    .execute(&state.pool)
                    .await
                    {
                        tracing::error!(idempotency_key = %key, error = %db, "failed to mark escrow action 'error' after broadcast failure");
                    }
                    return Err(e);
                }
            }
        }
        None => {
            let tx_hash = signer.send_direct_call(call.to, call.data).await?;
            sqlx::query(
                "INSERT INTO escrow_actions \
                 (tx_hash, action, collection, token_id, buyer, escrow_address, chain_id, status) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'sent')",
            )
            .bind(&tx_hash)
            .bind(action.as_str())
            .bind(collection_hex)
            .bind(token_id_text)
            .bind(buyer_hex)
            .bind(&escrow_hex)
            .bind(chain_id)
            .execute(&state.pool)
            .await?;
            tx_hash
        }
    };

    tracing::info!(
        action = action.as_str(),
        tx_hash = %tx_hash,
        collection = %collection_hex,
        escrow = %escrow_hex,
        "escrow action broadcast + recorded"
    );
    Ok(tx_hash)
}
