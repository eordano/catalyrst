use alloy::primitives::{Address, U256};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::require_admin;
use crate::handlers::broker::{mark_error, set_buy_sent};
use crate::handlers::{idempotency_key, require_broadcast_enabled};
use crate::http::errors::ApiError;
use crate::ports::broker::{
    build_mana_approve, build_name_mint, build_name_secondary, build_name_transfer,
    minted_token_id_from_logs, parse_address, parse_token_id, parse_wei, parse_wei_allow_zero,
    BrokerCall, PurchaseMode,
};
use crate::ports::contracts_addrs::NameContracts;
use crate::ports::signer::{DirectSigner, ReceiptOutcome};
use crate::AppState;

fn name_mint_price() -> U256 {
    U256::from(100_000_000_000_000_000_000u128)
}

fn eth_signer(state: &AppState) -> Result<&DirectSigner, ApiError> {
    require_broadcast_enabled(state, "name operations")?;
    state.eth_signer.as_ref().ok_or_else(|| {
        ApiError::RelayerUnavailable(
            "Name operations require the Ethereum signer (set META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + ETH_RPC_URL); nothing was broadcast.".into(),
        )
    })
}

fn name_contracts(state: &AppState) -> Result<NameContracts, ApiError> {
    let chain_id = state.config.names_chain_id;
    NameContracts::for_chain(chain_id).ok_or_else(|| {
        ApiError::InvalidTransaction(format!(
            "no Decentraland NAME contracts known for chain {chain_id}"
        ))
    })
}

fn enforce_names_price_cap(state: &AppState, price_wei: U256) -> Result<(), ApiError> {
    let Some(cap) = state.config.names_max_price_wei else {
        return Ok(());
    };
    if price_wei > cap {
        return Err(ApiError::Forbidden(format!(
            "name price {price_wei} wei exceeds the configured NAMES_MAX_PRICE_WEI cap {cap}; raise the cap if intentional"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameBuyBody {
    pub mode: String,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub token_id: Option<String>,

    #[serde(default)]
    pub price_wei: Option<String>,

    pub buyer_address: String,

    #[serde(default)]
    pub beneficiary_address: Option<String>,

    #[serde(default)]
    pub idempotency_key: Option<String>,
}

pub async fn buy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<NameBuyBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let mode = PurchaseMode::parse_name(body.mode.trim())?;
    let contracts = name_contracts(&state)?;
    let registrar = contracts.registrar;
    let buyer = parse_address("buyerAddress", &body.buyer_address)?;
    let chain_id = state.config.names_chain_id;

    let signer = eth_signer(&state)?;
    let relayer = signer.relayer_address();

    let beneficiary = match (mode, body.beneficiary_address.as_deref()) {
        (PurchaseMode::NameMint, Some(b)) => parse_address("beneficiaryAddress", b)?,
        (PurchaseMode::NameSecondary, Some(_)) => {
            return Err(ApiError::InvalidTransaction(
                "secondary name buys always custody in the relayer (the marketplace delivers to the caller); deliver afterwards via POST /broker/names/transfer".into(),
            ))
        }
        _ => relayer,
    };

    let (call, name_for_row, token_id_for_row, price_wei) = match mode {
        PurchaseMode::NameMint => {
            let name = body
                .name
                .as_deref()
                .ok_or_else(|| ApiError::InvalidTransaction("mint requires `name`".into()))?;
            let price = name_mint_price();
            if let Some(p) = body.price_wei.as_deref() {
                if parse_wei_allow_zero("priceWei", p)? != price {
                    return Err(ApiError::InvalidTransaction(format!(
                        "mint price is fixed by DCLControllerV2 at {price} wei (100 MANA); got {p:?}"
                    )));
                }
            }
            enforce_names_price_cap(&state, price)?;
            (
                build_name_mint(&contracts, name, beneficiary)?,
                Some(name.to_string()),
                None,
                price,
            )
        }
        _ => {
            let token_id_raw = body.token_id.as_deref().ok_or_else(|| {
                ApiError::InvalidTransaction("secondary name buy requires `tokenId`".into())
            })?;
            let token_id = parse_token_id(token_id_raw)?;
            let price_raw = body.price_wei.as_deref().ok_or_else(|| {
                ApiError::InvalidTransaction("secondary name buy requires `priceWei`".into())
            })?;
            let price = parse_wei("priceWei", price_raw)?;
            enforce_names_price_cap(&state, price)?;
            (
                build_name_secondary(&contracts, token_id, price),
                None,
                Some(token_id.to_string()),
                price,
            )
        }
    };

    let Some(key) = idempotency_key(&headers, body.idempotency_key.as_deref()) else {
        return Err(ApiError::InvalidTransaction(
            "Idempotency-Key is required for name buys (funds safety: it prevents a retry from re-buying)".into(),
        ));
    };

    let registrar_hex = format!("{registrar:#x}");
    let custody_hex = format!("{beneficiary:#x}");
    let buyer_hex = format!("{buyer:#x}");

    let claim = sqlx::query(
        "INSERT INTO broker_purchases \
         (idempotency_key, collection, item_id, token_id, buyer_address, escrow_address, price_wei, chain_id, mode, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8, $9, 'pending') \
         ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING",
    )
    .bind(&key)
    .bind(&registrar_hex)
    .bind(name_for_row.as_deref())
    .bind(token_id_for_row.as_deref())
    .bind(&buyer_hex)
    .bind(&custody_hex)
    .bind(price_wei.to_string())
    .bind(chain_id as i64)
    .bind(mode.as_str())
    .execute(&state.pool)
    .await?;

    let tx_hash = if claim.rows_affected() == 0 {
        resume_name_buy(&state, signer, &key, mode, registrar, beneficiary).await?
    } else {
        drive_name_buy(&state, signer, &key, mode, call, registrar, beneficiary).await?
    };

    tracing::info!(
        idempotency_key = %key,
        tx_hash = %tx_hash,
        mode = mode.as_str(),
        registrar = %registrar_hex,
        custody = %custody_hex,
        buyer = %buyer_hex,
        "name buy confirmed"
    );

    Ok(Json(json!({
        "ok": true,
        "txHash": tx_hash,
        "mode": mode.as_str(),
    })))
}

async fn drive_name_buy(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    call: BrokerCall,
    registrar: Address,
    beneficiary: Address,
) -> Result<String, ApiError> {
    let tx = match signer.send_direct_call(call.to, call.data).await {
        Ok(h) => h,
        Err(e) => {
            mark_error(state, key).await;
            return Err(e);
        }
    };
    set_buy_sent(state, key, &tx).await?;

    confirm_name_buy(state, signer, key, mode, registrar, beneficiary, &tx).await?;
    Ok(tx)
}

async fn confirm_name_buy(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    registrar: Address,
    beneficiary: Address,
    tx: &str,
) -> Result<(), ApiError> {
    let (outcome, logs) = signer.await_receipt_detailed(tx).await?;
    match outcome {
        ReceiptOutcome::Confirmed => {
            let minted = match mode {
                PurchaseMode::NameMint => minted_token_id_from_logs(&logs, registrar, beneficiary),
                _ => None,
            };
            sqlx::query(
                "UPDATE broker_purchases SET status = 'confirmed', minted_token_id = COALESCE($2, minted_token_id), updated_at = NOW() \
                 WHERE idempotency_key = $1",
            )
            .bind(key)
            .bind(minted.map(|t| t.to_string()))
            .execute(&state.pool)
            .await?;
            Ok(())
        }
        ReceiptOutcome::Reverted => {
            set_buy_status_if(state, key, "sent", "reverted").await;
            Err(ApiError::RelayReverted(format!(
                "name buy tx {tx} reverted on-chain; no MANA spent, no NAME acquired"
            )))
        }
        ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
            "name buy tx {tx} not yet mined; not confirming — retry/reconcile to settle"
        ))),
    }
}

async fn resume_name_buy(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    registrar: Address,
    beneficiary: Address,
) -> Result<String, ApiError> {
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, tx_hash FROM broker_purchases WHERE idempotency_key = $1")
            .bind(key)
            .fetch_one(&state.pool)
            .await?;
    let (status, tx_hash) = row;

    match status.as_str() {
        "confirmed" => {
            let tx = tx_hash.ok_or_else(|| {
                ApiError::Internal(format!("name buy {key:?} is 'confirmed' without a tx_hash"))
            })?;
            tracing::info!(idempotency_key = %key, tx_hash = %tx, "name buy idempotent replay -> recorded txHash");
            Ok(tx)
        }
        "reverted" => Err(ApiError::RelayReverted(format!(
            "name buy {key:?} reverted on a prior attempt; not re-broadcasting"
        ))),
        "error" => {
            let rearmed = sqlx::query(
                "UPDATE broker_purchases SET status = 'pending', updated_at = NOW() \
                 WHERE idempotency_key = $1 AND status = 'error'",
            )
            .bind(key)
            .execute(&state.pool)
            .await?;
            if rearmed.rows_affected() == 0 {
                return Err(ApiError::Conflict(format!(
                    "name buy {key:?} changed state concurrently; retry"
                )));
            }
            Err(ApiError::Conflict(format!(
                "name buy {key:?} previously failed before broadcast and was re-armed; retry to re-broadcast"
            )))
        }
        "sent" => {
            let tx = tx_hash.ok_or_else(|| {
                ApiError::Internal(format!("name buy {key:?} is 'sent' without a tx_hash"))
            })?;
            confirm_name_buy(state, signer, key, mode, registrar, beneficiary, &tx).await?;
            Ok(tx)
        }
        "pending" => Err(ApiError::Conflict(format!(
            "name buy {key:?} is in flight (status 'pending'); not re-broadcasting — retry once it settles"
        ))),
        other => Err(ApiError::Conflict(format!(
            "name buy {key:?} has unexpected status {other:?}; reconcile manually"
        ))),
    }
}

async fn set_buy_status_if(state: &AppState, key: &str, from: &str, to: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE broker_purchases SET status = $3, updated_at = NOW() \
         WHERE idempotency_key = $1 AND status = $2",
    )
    .bind(key)
    .bind(from)
    .bind(to)
    .execute(&state.pool)
    .await
    {
        tracing::error!(idempotency_key = %key, error = %e, "failed to mark name buy '{to}'");
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameTransferBody {
    pub token_id: String,
    pub to_address: String,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

pub async fn transfer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<NameTransferBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let contracts = name_contracts(&state)?;
    let registrar = contracts.registrar;
    let token_id = parse_token_id(&body.token_id)?;
    let to = parse_address("toAddress", &body.to_address)?;
    let chain_id = state.config.names_chain_id;

    let signer = eth_signer(&state)?;
    let relayer = signer.relayer_address();
    if to == relayer {
        return Err(ApiError::InvalidTransaction(
            "toAddress is the relayer itself; nothing to transfer".into(),
        ));
    }

    let Some(key) = idempotency_key(&headers, body.idempotency_key.as_deref()) else {
        return Err(ApiError::InvalidTransaction(
            "Idempotency-Key is required for name transfers (a retry must not double-send)".into(),
        ));
    };

    let claim = sqlx::query(
        "INSERT INTO name_transfers \
         (idempotency_key, registrar, token_id, from_address, to_address, chain_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending') \
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(&key)
    .bind(format!("{registrar:#x}"))
    .bind(token_id.to_string())
    .bind(format!("{relayer:#x}"))
    .bind(format!("{to:#x}"))
    .bind(chain_id as i64)
    .execute(&state.pool)
    .await?;

    if claim.rows_affected() == 0 {
        if let Some(tx) = resume_transfer(&state, signer, &key).await? {
            return Ok(Json(json!({ "ok": true, "txHash": tx })));
        }
    }

    let call = build_name_transfer(registrar, relayer, to, token_id);
    let tx = match signer.send_direct_call(call.to, call.data).await {
        Ok(h) => h,
        Err(e) => {
            set_transfer_status_if(&state, &key, "pending", "error").await;
            return Err(e);
        }
    };
    sqlx::query(
        "UPDATE name_transfers SET tx_hash = $2, status = 'sent', updated_at = NOW() WHERE idempotency_key = $1",
    )
    .bind(&key)
    .bind(&tx)
    .execute(&state.pool)
    .await?;

    settle_transfer(&state, signer, &key, &tx).await?;
    tracing::info!(idempotency_key = %key, tx_hash = %tx, to = %to, "name transfer confirmed");
    Ok(Json(json!({ "ok": true, "txHash": tx })))
}

async fn resume_transfer(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
) -> Result<Option<String>, ApiError> {
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, tx_hash FROM name_transfers WHERE idempotency_key = $1")
            .bind(key)
            .fetch_one(&state.pool)
            .await?;
    let (status, tx_hash) = row;

    match status.as_str() {
        "confirmed" => {
            let tx = tx_hash.ok_or_else(|| {
                ApiError::Internal(format!(
                    "name transfer {key:?} is 'confirmed' without a tx_hash"
                ))
            })?;
            tracing::info!(idempotency_key = %key, tx_hash = %tx, "name transfer idempotent replay -> recorded txHash");
            Ok(Some(tx))
        }
        "reverted" => Err(ApiError::RelayReverted(format!(
            "name transfer {key:?} reverted on a prior attempt; not re-broadcasting"
        ))),
        "sent" => {
            let tx = tx_hash.ok_or_else(|| {
                ApiError::Internal(format!("name transfer {key:?} is 'sent' without a tx_hash"))
            })?;
            settle_transfer(state, signer, key, &tx).await?;
            Ok(Some(tx))
        }
        "error" => {
            let rearmed = sqlx::query(
                "UPDATE name_transfers SET status = 'pending', updated_at = NOW() \
                 WHERE idempotency_key = $1 AND status = 'error'",
            )
            .bind(key)
            .execute(&state.pool)
            .await?;
            if rearmed.rows_affected() == 0 {
                return Err(ApiError::Conflict(format!(
                    "name transfer {key:?} changed state concurrently; retry"
                )));
            }
            tracing::info!(idempotency_key = %key, "re-arming errored name transfer for re-broadcast (prior attempt failed pre-broadcast; no NFT moved)");
            Ok(None)
        }
        "pending" => Err(ApiError::Conflict(format!(
            "name transfer {key:?} is in flight (status 'pending'); retry once it settles"
        ))),
        other => Err(ApiError::Conflict(format!(
            "name transfer {key:?} has unexpected status {other:?}; reconcile manually"
        ))),
    }
}

async fn settle_transfer(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    tx: &str,
) -> Result<(), ApiError> {
    match signer.await_receipt(tx).await? {
        ReceiptOutcome::Confirmed => {
            set_transfer_status_if(state, key, "sent", "confirmed").await;
            Ok(())
        }
        ReceiptOutcome::Reverted => {
            set_transfer_status_if(state, key, "sent", "reverted").await;
            Err(ApiError::RelayReverted(format!(
                "name transfer tx {tx} reverted on-chain (NAME still in relayer custody)"
            )))
        }
        ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
            "name transfer tx {tx} not yet mined; retry to settle"
        ))),
    }
}

async fn set_transfer_status_if(state: &AppState, key: &str, from: &str, to: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE name_transfers SET status = $3, updated_at = NOW() \
         WHERE idempotency_key = $1 AND status = $2",
    )
    .bind(key)
    .bind(from)
    .bind(to)
    .execute(&state.pool)
    .await
    {
        tracing::error!(idempotency_key = %key, error = %e, "failed to mark name transfer '{to}'");
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveManaBody {
    pub spender: String,
    #[serde(default)]
    pub amount_wei: Option<String>,
}

pub async fn approve_mana(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ApproveManaBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let contracts = name_contracts(&state)?;
    let spender = match body.spender.trim() {
        "controller" => contracts.controller_v2,
        "marketplace" => contracts.marketplace,
        other => {
            return Err(ApiError::InvalidTransaction(format!(
                "invalid spender {other:?}: expected \"controller\" or \"marketplace\""
            )))
        }
    };
    let amount = match body.amount_wei.as_deref() {
        Some(raw) => parse_wei("amountWei", raw)?,
        None => U256::MAX,
    };

    let signer = eth_signer(&state)?;
    let call = build_mana_approve(contracts.mana_token, spender, amount);
    let tx = signer.send_direct_call(call.to, call.data).await?;

    match signer.await_receipt(&tx).await? {
        ReceiptOutcome::Confirmed => {
            tracing::info!(tx_hash = %tx, spender = %spender, "MANA approve confirmed on names chain");
            Ok(Json(json!({ "ok": true, "txHash": tx })))
        }
        ReceiptOutcome::Reverted => Err(ApiError::RelayReverted(format!(
            "MANA approve tx {tx} reverted on-chain"
        ))),
        ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
            "MANA approve tx {tx} not yet mined; check before retrying"
        ))),
    }
}
