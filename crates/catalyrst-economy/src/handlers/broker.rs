use alloy::primitives::{Address, U256};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::require_admin;
use crate::handlers::{idempotency_key, require_broadcast_enabled};
use crate::http::errors::ApiError;
use crate::ports::broker::{
    build_forward_to_escrow, build_mana_approve, build_primary, build_secondary,
    minted_token_id_from_logs, parse_address, parse_item_id, parse_token_id, parse_wei,
    parse_wei_allow_zero, BrokerCall, PurchaseMode,
};
use crate::ports::contracts_addrs::DclContracts;
use crate::ports::signer::{DirectSigner, ReceiptOutcome};
use crate::ports::trade::{
    build_trade_accept, transferred_token_id_from_logs, validate_trade, TradeDelivery,
    TradeExpectations, TradeIn, ValidatedTrade,
};
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyBody {
    pub collection: String,

    #[serde(default)]
    pub item_id: Option<String>,

    #[serde(default)]
    pub token_id: Option<String>,

    pub price_wei: String,

    pub escrow_address: String,

    pub mode: String,

    #[serde(default)]
    pub buyer_address: Option<String>,

    /// mode "trade": the FULL signed-trade payload (id, signer, signature,
    /// checks, sent, received — the catalyrst-market GET /v1/trades/{id}
    /// shape). The economy service has no market-DB access by design.
    #[serde(default)]
    pub trade: Option<serde_json::Value>,

    #[serde(default)]
    pub idempotency_key: Option<String>,
}

struct BuyOutcome {
    forward_tx_hash: String,
}

pub async fn buy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<BuyBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let mode = PurchaseMode::parse_wearable(body.mode.trim())?;
    let collection = parse_address("collection", &body.collection)?;
    let escrow = parse_address("escrowAddress", &body.escrow_address)?;
    let price_wei = match mode {
        PurchaseMode::Primary => parse_wei_allow_zero("priceWei", &body.price_wei)?,
        _ => parse_wei("priceWei", &body.price_wei)?,
    };
    if mode != PurchaseMode::Trade && body.trade.is_some() {
        return Err(ApiError::InvalidTransaction(
            "a `trade` payload was sent but mode is not \"trade\"".into(),
        ));
    }

    let buyer = match body.buyer_address.as_deref() {
        Some(b) => parse_address("buyerAddress", b)?,
        None => {
            return Err(ApiError::InvalidTransaction(
                "buyerAddress is required: the buyer is ABI-encoded into the escrow's onERC721Received _data so the 15-day lease can be recorded".into(),
            ))
        }
    };

    if let Some(pinned_raw) = state.config.landiler_escrow_address.as_deref() {
        let pinned = parse_address("LANDILER_ESCROW_ADDRESS", pinned_raw).map_err(|_| {
            ApiError::Internal("LANDILER_ESCROW_ADDRESS is set but is not a valid address".into())
        })?;
        if escrow != pinned {
            return Err(ApiError::Forbidden(format!(
                "escrowAddress {escrow:#x} is not the configured LANDILER_ESCROW_ADDRESS; broker buys may only deliver to the pinned escrow"
            )));
        }
    }

    let chain_id = state.config.collections_chain_id;
    let contracts = DclContracts::for_chain(chain_id).ok_or_else(|| {
        ApiError::InvalidTransaction(format!(
            "no Decentraland contracts known for chain {chain_id}"
        ))
    })?;

    require_broadcast_enabled(&state, "broker buys")?;

    let Some(signer) = state.transaction.direct_signer() else {
        return Err(ApiError::RelayerUnavailable(
            "Broker buy requires the direct JSON-RPC signer (set META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + RPC_URL). Validation passed; broadcast is unavailable.".into(),
        ));
    };
    let relayer = signer.relayer_address();

    // mode "trade": validate the signed payload fail-closed (venue, chain,
    // single MANA<->NFT/item shape, checks window, no allowlists/external
    // checks, exact pinned price, and local EIP-712 signature recovery)
    // BEFORE claiming the idempotency key or touching the chain.
    let validated_trade: Option<ValidatedTrade> = match mode {
        PurchaseMode::Trade => {
            let raw = body.trade.as_ref().ok_or_else(|| {
                ApiError::InvalidTransaction(
                    "mode \"trade\" requires the full `trade` payload (id, signer, signature, \
                     checks, sent, received)"
                        .into(),
                )
            })?;
            let parsed: TradeIn = serde_json::from_value(raw.clone()).map_err(|e| {
                ApiError::InvalidTransaction(format!("malformed trade payload: {e}"))
            })?;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(validate_trade(
                &parsed,
                &TradeExpectations {
                    collection,
                    price_wei,
                    chain_id,
                    relayer,
                    now_ms,
                },
            )?)
        }
        _ => None,
    };

    let token_id_for_secondary: Option<U256> = match mode {
        PurchaseMode::Primary => {
            if body.item_id.as_deref().is_none() {
                return Err(ApiError::InvalidTransaction(
                    "primary buy requires `itemId`".into(),
                ));
            }
            None
        }
        PurchaseMode::Trade => match validated_trade.as_ref().map(|v| v.delivery) {
            Some(TradeDelivery::Nft { token_id }) => Some(token_id),
            // item-order trades mint: the tokenId is only known from the receipt
            _ => None,
        },
        _ => {
            let token_id_raw = body.token_id.as_deref().ok_or_else(|| {
                ApiError::InvalidTransaction("secondary buy requires `tokenId`".into())
            })?;
            Some(parse_token_id(token_id_raw)?)
        }
    };

    let buy_call: BrokerCall = match mode {
        PurchaseMode::Primary => {
            let item_id = parse_item_id(body.item_id.as_deref().unwrap())?;
            build_primary(&contracts, collection, item_id, price_wei, relayer)
        }
        PurchaseMode::Trade => build_trade_accept(validated_trade.as_ref().unwrap(), relayer),
        _ => build_secondary(
            &contracts,
            collection,
            token_id_for_secondary.unwrap(),
            price_wei,
        ),
    };

    let Some(key) = idempotency_key(&headers, body.idempotency_key.as_deref()) else {
        return Err(ApiError::InvalidTransaction(
            "Idempotency-Key is required for broker buys (funds safety: it prevents a retry from re-buying)".into(),
        ));
    };

    let collection_hex = format!("{collection:#x}");
    let escrow_hex = format!("{escrow:#x}");
    let buyer_hex = format!("{buyer:#x}");
    let price_text = price_wei.to_string();
    let token_id_text = token_id_for_secondary.map(|t| t.to_string());
    // For item-order trades record the item id from the signed payload so the
    // row is self-describing even when the caller omitted `itemId`.
    let item_id_text: Option<String> = match validated_trade.as_ref().map(|v| v.delivery) {
        Some(TradeDelivery::Item { item_id }) => Some(item_id.to_string()),
        _ => body.item_id.clone(),
    };
    let trade_sig_hash = validated_trade
        .as_ref()
        .map(|v| format!("{:#x}", v.hashed_signature));

    let claim = sqlx::query(
        "INSERT INTO broker_purchases \
         (idempotency_key, collection, item_id, token_id, buyer_address, escrow_address, price_wei, chain_id, mode, trade_hashed_signature, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8, $9, $10, 'pending') \
         ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING",
    )
    .bind(&key)
    .bind(&collection_hex)
    .bind(item_id_text.as_deref())
    .bind(token_id_text.as_deref())
    .bind(&buyer_hex)
    .bind(&escrow_hex)
    .bind(&price_text)
    .bind(chain_id as i64)
    .bind(mode.as_str())
    .bind(trade_sig_hash.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| match &e {
        // A DIFFERENT idempotency key already claimed this trade signature
        // (broker_purchases_trade_sig_uidx): a 1-use signed listing cannot be
        // sold twice — terminal, do not burn gas on a guaranteed revert.
        sqlx::Error::Database(db)
            if db.constraint() == Some("broker_purchases_trade_sig_uidx") =>
        {
            ApiError::InvalidTransaction(
                "this signed trade is already being (or has been) purchased under another \
                 request; a 1-use trade listing cannot be sold twice"
                    .into(),
            )
        }
        _ => ApiError::from(e),
    })?;

    let outcome = if claim.rows_affected() == 0 {
        resume_existing(
            &state,
            signer,
            &key,
            mode,
            collection,
            escrow,
            buyer,
            relayer,
            validated_trade.as_ref(),
        )
        .await?
    } else {
        drive_buy_then_forward(
            &state,
            signer,
            &key,
            mode,
            buy_call,
            collection,
            escrow,
            buyer,
            relayer,
            token_id_for_secondary,
            validated_trade.as_ref(),
        )
        .await?
    };

    tracing::info!(
        idempotency_key = %key,
        forward_tx_hash = %outcome.forward_tx_hash,
        mode = mode.as_str(),
        collection = %collection_hex,
        escrow = %escrow_hex,
        "broker buy confirmed in escrow custody"
    );

    Ok(Json(json!({
        "ok": true,
        "txHash": outcome.forward_tx_hash,
        "mode": mode.as_str(),
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveCollectionsManaBody {
    pub spender: String,
    #[serde(default)]
    pub amount_wei: Option<String>,
}

pub(crate) fn collections_spender(
    contracts: &DclContracts,
    raw: &str,
) -> Result<Address, ApiError> {
    match raw.trim() {
        "collection-store" => Ok(contracts.collection_store),
        "marketplace-v2" => Ok(contracts.marketplace_v2),
        // DecentralandMarketplacePolygon (off-chain signed trades): pulls the
        // MANA payment (price incl. fee/royalty split) from the relayer on
        // accept(), so it needs a MANA allowance before mode "trade" works.
        "offchain-marketplace" => Ok(contracts.offchain_marketplace),
        other => Err(ApiError::InvalidTransaction(format!(
            "invalid spender {other:?}: expected \"collection-store\", \"marketplace-v2\", or \
             \"offchain-marketplace\""
        ))),
    }
}

pub async fn approve_mana_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ApproveCollectionsManaBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;

    let chain_id = state.config.collections_chain_id;
    let contracts = DclContracts::for_chain(chain_id).ok_or_else(|| {
        ApiError::InvalidTransaction(format!(
            "no Decentraland contracts known for chain {chain_id}"
        ))
    })?;
    let spender = collections_spender(&contracts, &body.spender)?;
    let amount = match body.amount_wei.as_deref() {
        Some(raw) => parse_wei("amountWei", raw)?,
        None => U256::MAX,
    };

    require_broadcast_enabled(&state, "MANA approvals")?;
    let Some(signer) = state.transaction.direct_signer() else {
        return Err(ApiError::RelayerUnavailable(
            "MANA approve requires the direct JSON-RPC signer (set META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + RPC_URL); nothing was broadcast.".into(),
        ));
    };

    let call = build_mana_approve(contracts.mana_token, spender, amount);
    let tx = signer.send_direct_call(call.to, call.data).await?;

    match signer.await_receipt(&tx).await? {
        ReceiptOutcome::Confirmed => {
            tracing::info!(
                tx_hash = %tx,
                spender = %spender,
                chain_id,
                "MANA approve confirmed on collections chain"
            );
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

#[allow(clippy::too_many_arguments)]
async fn drive_buy_then_forward(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    buy_call: BrokerCall,
    collection: Address,
    escrow: Address,
    buyer: Address,
    relayer: Address,
    token_id_secondary: Option<U256>,
    trade: Option<&ValidatedTrade>,
) -> Result<BuyOutcome, ApiError> {
    let buy_tx = match signer.send_direct_call(buy_call.to, buy_call.data).await {
        Ok(h) => h,
        Err(e) => {
            mark_error(state, key).await;
            return Err(e);
        }
    };
    set_buy_sent(state, key, &buy_tx).await?;

    let token_id = confirm_buy(
        state,
        signer,
        key,
        mode,
        collection,
        relayer,
        &buy_tx,
        token_id_secondary,
        trade,
    )
    .await?;

    drive_forward(
        state, signer, key, collection, escrow, buyer, relayer, token_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn confirm_buy(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    collection: Address,
    relayer: Address,
    buy_tx: &str,
    token_id_secondary: Option<U256>,
    trade: Option<&ValidatedTrade>,
) -> Result<U256, ApiError> {
    let (outcome, logs) = signer.await_receipt_detailed(buy_tx).await?;
    match outcome {
        ReceiptOutcome::Confirmed => {
            let token_id = match mode {
                PurchaseMode::Primary => minted_token_id_from_logs(&logs, collection, relayer)
                    .ok_or_else(|| {
                        ApiError::RelayerFailed(
                            "primary buy confirmed but no ERC-721 Transfer-to-relayer log found; cannot determine the minted tokenId to forward".into(),
                        )
                    })?,
                PurchaseMode::Trade => {
                    trade_delivered_token_id(&logs, collection, relayer, trade, token_id_secondary)?
                }
                _ => token_id_secondary.ok_or_else(|| {
                    ApiError::Internal("secondary buy confirmed without a tokenId".into())
                })?,
            };
            set_bought(state, key, &token_id.to_string()).await?;
            Ok(token_id)
        }
        ReceiptOutcome::Reverted => {
            set_reverted(state, key).await;
            Err(ApiError::RelayReverted(format!(
                "broker buy tx {buy_tx} reverted on-chain; no MANA spent, no item minted"
            )))
        }
        ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
            "broker buy tx {buy_tx} not yet mined; not confirming — retry/reconcile to settle"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn drive_forward(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    collection: Address,
    escrow: Address,
    buyer: Address,
    relayer: Address,
    token_id: U256,
) -> Result<BuyOutcome, ApiError> {
    let fwd = build_forward_to_escrow(collection, relayer, escrow, token_id, buyer);
    let fwd_tx = signer.send_direct_call(fwd.to, fwd.data).await?;
    set_forwarding(state, key, &fwd_tx).await?;

    match signer.await_receipt(&fwd_tx).await? {
        ReceiptOutcome::Confirmed => {
            set_confirmed(state, key).await?;
            Ok(BuyOutcome {
                forward_tx_hash: fwd_tx,
            })
        }
        ReceiptOutcome::Reverted => {
            set_reverted(state, key).await;
            Err(ApiError::RelayReverted(format!(
                "escrow forward tx {fwd_tx} reverted on-chain (token at relayer, not in custody)"
            )))
        }
        ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
            "escrow forward tx {fwd_tx} not yet mined; not confirming — retry/reconcile to settle"
        ))),
    }
}

/// Receipt classification for mode "trade": the accepted trade's delivery is
/// the ERC721 Transfer(seller -> relayer) for nft orders, or the mint
/// Transfer(0x0 -> relayer) for item orders (the collection issues a token).
fn trade_delivered_token_id(
    logs: &[crate::ports::signer::ReceiptLog],
    collection: Address,
    relayer: Address,
    trade: Option<&ValidatedTrade>,
    expected_token: Option<U256>,
) -> Result<U256, ApiError> {
    let trade = trade.ok_or_else(|| {
        ApiError::Internal("trade buy confirmed without the validated trade context".into())
    })?;
    let from = match trade.delivery {
        TradeDelivery::Nft { .. } => trade.signer,
        TradeDelivery::Item { .. } => Address::ZERO,
    };
    let delivered = transferred_token_id_from_logs(logs, collection, from, relayer)
        .ok_or_else(|| {
            ApiError::RelayerFailed(format!(
                "trade accept confirmed but no ERC-721 Transfer({from:#x} -> relayer) log \
                 found on {collection:#x}; cannot determine the delivered tokenId to forward"
            ))
        })?;
    if let Some(expected) = expected_token {
        if delivered != expected {
            return Err(ApiError::RelayerFailed(format!(
                "trade accept delivered tokenId {delivered}, expected {expected}; refusing to \
                 forward the wrong token"
            )));
        }
    }
    Ok(delivered)
}

#[allow(clippy::too_many_arguments)]
async fn resume_existing(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    collection: Address,
    escrow: Address,
    buyer: Address,
    relayer: Address,
    trade: Option<&ValidatedTrade>,
) -> Result<BuyOutcome, ApiError> {
    let row: ExistingRow = sqlx::query_as(
        "SELECT status, tx_hash, forward_tx_hash, token_id, minted_token_id \
         FROM broker_purchases WHERE idempotency_key = $1",
    )
    .bind(key)
    .fetch_one(&state.pool)
    .await?;

    match row.status.as_str() {
        "confirmed" => {
            let fwd = row.forward_tx_hash.ok_or_else(|| {
                ApiError::Internal(format!(
                    "broker buy {key:?} is 'confirmed' but has no forward_tx_hash"
                ))
            })?;
            tracing::info!(idempotency_key = %key, forward_tx_hash = %fwd, "broker buy idempotent replay -> recorded forward txHash");
            Ok(BuyOutcome {
                forward_tx_hash: fwd,
            })
        }
        "reverted" => Err(ApiError::RelayReverted(format!(
            "broker buy {key:?} reverted on a prior attempt; not re-broadcasting"
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
                    "broker buy {key:?} changed state concurrently; retry"
                )));
            }
            Err(ApiError::Conflict(format!(
                "broker buy {key:?} previously failed before broadcast and was re-armed; retry to re-broadcast"
            )))
        }
        "sent" => {
            let buy_tx = row.tx_hash.ok_or_else(|| {
                ApiError::Internal(format!("broker buy {key:?} is 'sent' without a tx_hash"))
            })?;
            let token_id_secondary = match (mode, row.token_id.as_deref()) {
                (PurchaseMode::Secondary, Some(t)) | (PurchaseMode::Trade, Some(t)) => {
                    Some(parse_token_id(t)?)
                }
                _ => None,
            };
            let token_id = confirm_buy(
                state,
                signer,
                key,
                mode,
                collection,
                relayer,
                &buy_tx,
                token_id_secondary,
                trade,
            )
            .await?;
            drive_forward(state, signer, key, collection, escrow, buyer, relayer, token_id).await
        }
        "bought" => {
            let token_id =
                resume_token_id(state, signer, key, mode, collection, relayer, &row, trade)
                    .await?;
            drive_forward(state, signer, key, collection, escrow, buyer, relayer, token_id).await
        }
        "forwarding" => {
            let fwd_tx = row.forward_tx_hash.clone().ok_or_else(|| {
                ApiError::Internal(format!(
                    "broker buy {key:?} is 'forwarding' without a forward_tx_hash"
                ))
            })?;
            match signer.await_receipt(&fwd_tx).await? {
                ReceiptOutcome::Confirmed => {
                    set_confirmed(state, key).await?;
                    Ok(BuyOutcome {
                        forward_tx_hash: fwd_tx,
                    })
                }
                ReceiptOutcome::Reverted => {
                    set_reverted(state, key).await;
                    Err(ApiError::RelayReverted(format!(
                        "escrow forward tx {fwd_tx} reverted on-chain"
                    )))
                }
                ReceiptOutcome::Pending => Err(ApiError::RelayerTimeout(format!(
                    "escrow forward tx {fwd_tx} not yet mined; retry/reconcile to settle"
                ))),
            }
        }
        "pending" => Err(ApiError::Conflict(format!(
            "broker buy {key:?} is in flight (status 'pending'); not re-broadcasting — retry once it settles"
        ))),
        other => Err(ApiError::Conflict(format!(
            "broker buy {key:?} has unexpected status {other:?}; reconcile manually"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn resume_token_id(
    state: &AppState,
    signer: &DirectSigner,
    key: &str,
    mode: PurchaseMode,
    collection: Address,
    relayer: Address,
    row: &ExistingRow,
    trade: Option<&ValidatedTrade>,
) -> Result<U256, ApiError> {
    match mode {
        PurchaseMode::Trade => {
            // set_bought recorded the delivered token; nft-order claims also
            // recorded token_id up-front. Fall back to receipt re-extraction
            // (item orders interrupted between 'bought' write and here).
            if let Some(t) = row.minted_token_id.as_deref() {
                return parse_token_id(t);
            }
            if let Some(t) = row.token_id.as_deref() {
                return parse_token_id(t);
            }
            let buy_tx = row.tx_hash.as_deref().ok_or_else(|| {
                ApiError::Internal(format!("trade broker buy {key:?} 'bought' without tx_hash"))
            })?;
            let (outcome, logs) = signer.await_receipt_detailed(buy_tx).await?;
            if outcome != ReceiptOutcome::Confirmed {
                return Err(ApiError::RelayerTimeout(format!(
                    "trade broker buy {key:?} 'bought' but the accept receipt is not \
                     confirmed on re-poll"
                )));
            }
            let token_id = trade_delivered_token_id(&logs, collection, relayer, trade, None)?;
            set_bought(state, key, &token_id.to_string()).await?;
            Ok(token_id)
        }
        PurchaseMode::Primary => {
            if let Some(t) = row.minted_token_id.as_deref() {
                return parse_token_id(t);
            }
            let buy_tx = row.tx_hash.as_deref().ok_or_else(|| {
                ApiError::Internal(format!(
                    "primary broker buy {key:?} 'bought' without tx_hash"
                ))
            })?;
            let (outcome, logs) = signer.await_receipt_detailed(buy_tx).await?;
            if outcome != ReceiptOutcome::Confirmed {
                return Err(ApiError::RelayerTimeout(format!(
                    "primary broker buy {key:?} 'bought' but buy receipt not confirmed on re-poll"
                )));
            }
            let token_id = minted_token_id_from_logs(&logs, collection, relayer).ok_or_else(|| {
                ApiError::RelayerFailed(
                    "primary buy confirmed but no Transfer-to-relayer log found to recover tokenId".into(),
                )
            })?;
            set_bought(state, key, &token_id.to_string()).await?;
            Ok(token_id)
        }
        _ => {
            let t = row.token_id.as_deref().ok_or_else(|| {
                ApiError::Internal(format!(
                    "secondary broker buy {key:?} 'bought' without token_id"
                ))
            })?;
            parse_token_id(t)
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExistingRow {
    status: String,
    tx_hash: Option<String>,
    forward_tx_hash: Option<String>,
    token_id: Option<String>,
    minted_token_id: Option<String>,
}

pub(crate) async fn set_buy_sent(
    state: &AppState,
    key: &str,
    buy_tx: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE broker_purchases SET tx_hash = $2, status = 'sent', updated_at = NOW() \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .bind(buy_tx)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn set_bought(state: &AppState, key: &str, token_id: &str) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE broker_purchases SET status = 'bought', minted_token_id = $2, updated_at = NOW() \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .bind(token_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn set_forwarding(state: &AppState, key: &str, fwd_tx: &str) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE broker_purchases SET status = 'forwarding', forward_tx_hash = $2, updated_at = NOW() \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .bind(fwd_tx)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn set_confirmed(state: &AppState, key: &str) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE broker_purchases SET status = 'confirmed', updated_at = NOW() \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn set_reverted(state: &AppState, key: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE broker_purchases SET status = 'reverted', updated_at = NOW() \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .execute(&state.pool)
    .await
    {
        tracing::error!(idempotency_key = %key, error = %e, "failed to mark broker buy 'reverted'");
    }
}

pub(crate) async fn mark_error(state: &AppState, key: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE broker_purchases SET status = 'error', updated_at = NOW() \
         WHERE idempotency_key = $1 AND status = 'pending'",
    )
    .bind(key)
    .execute(&state.pool)
    .await
    {
        tracing::error!(idempotency_key = %key, error = %e, "failed to mark broker buy 'error' after broadcast failure");
    }
}

#[cfg(test)]
mod tests {
    use super::collections_spender;
    use crate::ports::contracts_addrs::DclContracts;

    #[test]
    fn collections_spender_resolves_store_and_marketplace_only() {
        let c = DclContracts::for_chain(137).expect("polygon contracts");
        assert_eq!(
            collections_spender(&c, "collection-store").unwrap(),
            c.collection_store
        );
        assert_eq!(
            collections_spender(&c, " marketplace-v2 ").unwrap(),
            c.marketplace_v2
        );
        assert_eq!(
            collections_spender(&c, "offchain-marketplace").unwrap(),
            c.offchain_marketplace
        );
        assert_eq!(
            format!("{:#x}", collections_spender(&c, "offchain-marketplace").unwrap()),
            "0x540fb08edb56aae562864b390542c97f562825ba"
        );
        assert!(collections_spender(&c, "controller").is_err());
        assert!(
            collections_spender(&c, "0x214ffc0f0103735728dc66b61a22e4f163e275ae").is_err(),
            "raw addresses are refused: the allowance may only be armed for known contracts"
        );
        assert!(collections_spender(&c, "").is_err());
    }
}
