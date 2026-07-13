use alloy::primitives::{Address, B256, U256};
use alloy::sol_types::SolCall;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::admin::require_admin;
use crate::http::errors::ApiError;
use crate::ports::abi::{getNonceCall, ERC721_TRANSFER_TOPIC0};
use crate::ports::contracts_addrs::DclContracts;
use crate::ports::signer::{ReceiptLog, ReceiptOutcome};
use crate::AppState;

const ERC20_TRANSFER_TOPIC0: [u8; 32] = ERC721_TRANSFER_TOPIC0;

fn address_topic(a: Address) -> B256 {
    B256::from_slice(&[&[0u8; 12][..], a.as_slice()].concat())
}

pub fn sum_erc20_transfers_to(
    logs: &[ReceiptLog],
    token: Address,
    pay_to: Address,
) -> Option<(Address, U256)> {
    let topic0 = B256::from(ERC20_TRANSFER_TOPIC0);
    let to_topic = address_topic(pay_to);
    let mut found: Option<(Address, U256)> = None;
    for log in logs {
        if log.address != token
            || log.topics.len() != 3
            || log.topics[0] != topic0
            || log.topics[2] != to_topic
            || log.data.len() != 32
        {
            continue;
        }
        let from = Address::from_slice(&log.topics[1][12..]);
        let value = U256::from_be_slice(&log.data);
        match &mut found {
            None => found = Some((from, value)),
            Some((first_from, total)) if *first_from == from => {
                *total = total.saturating_add(value);
            }
            Some(_) => {
                tracing::warn!(
                    token = %token,
                    pay_to = %pay_to,
                    ignored_from = %from,
                    "receipt has MANA transfers to payTo from multiple senders; \
                     ignoring the extra sender's value"
                );
            }
        }
    }
    found
}

pub fn config_json(pay_to: Option<Address>, mana_token: Option<Address>, chain_id: u64) -> Value {
    let enabled = pay_to.is_some() && mana_token.is_some();
    json!({
        "payTo": pay_to.map(|a| format!("{a:#x}")),
        "manaToken": mana_token.map(|a| format!("{a:#x}")),
        "chainId": chain_id,
        "enabled": enabled,
    })
}

pub async fn config(State(state): State<AppState>) -> Json<Value> {
    let chain_id = state.config.collections_chain_id;
    let pay_to = state
        .transaction
        .direct_signer()
        .map(|s| s.relayer_address());
    let mana_token = DclContracts::for_chain(chain_id).map(|c| c.mana_token);
    Json(config_json(pay_to, mana_token, chain_id))
}

pub fn encode_get_nonce(user: Address) -> Vec<u8> {
    getNonceCall { user }.abi_encode()
}

pub fn decode_get_nonce_return(ret: &[u8]) -> Result<U256, ApiError> {
    getNonceCall::abi_decode_returns(ret)
        .map_err(|e| ApiError::RelayerFailed(format!("could not decode getNonce return: {e}")))
}

pub async fn nonce(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let user: Address = address
        .trim()
        .parse()
        .map_err(|e| ApiError::InvalidTransaction(format!("invalid address {address:?}: {e}")))?;

    let Some(signer) = state.transaction.direct_signer() else {
        return Err(ApiError::RelayerUnavailable(
            "No RPC provider is provisioned (META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + RPC_URL required); cannot read the meta-tx nonce.".into(),
        ));
    };
    let chain_id = state.config.collections_chain_id;
    let Some(contracts) = DclContracts::for_chain(chain_id) else {
        return Err(ApiError::RelayerUnavailable(format!(
            "no Decentraland contracts known for chain {chain_id}; cannot read the meta-tx nonce"
        )));
    };

    let ret = signer
        .eth_call(contracts.mana_token, encode_get_nonce(user).into())
        .await?;
    let nonce = decode_get_nonce_return(&ret)?;
    Ok(Json(json!({ "nonce": nonce.to_string() })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyBody {
    pub tx_hash: String,
}

fn validate_tx_hash(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    let hex = s.strip_prefix("0x").unwrap_or("");
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::InvalidTransaction(format!(
            "invalid txHash {raw:?}: expected 0x + 64 hex chars"
        )));
    }
    Ok(s.to_lowercase())
}

pub async fn verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<VerifyBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&state, &headers)?;
    let Json(body) = body.map_err(|e| ApiError::MalformedBody(e.body_text()))?;
    let tx_hash = validate_tx_hash(&body.tx_hash)?;

    let Some(signer) = state.transaction.direct_signer() else {
        return Err(ApiError::RelayerUnavailable(
            "No RPC provider is provisioned (META_TX_BROADCAST_ENABLED=true with RELAYER_PRIVATE_KEY + RPC_URL required); cannot verify payments.".into(),
        ));
    };
    let chain_id = state.config.collections_chain_id;
    let Some(contracts) = DclContracts::for_chain(chain_id) else {
        return Err(ApiError::RelayerUnavailable(format!(
            "no Decentraland contracts known for chain {chain_id}; cannot verify payments"
        )));
    };
    let pay_to = signer.relayer_address();

    let (outcome, logs) = signer.fetch_receipt_once(&tx_hash).await?;
    let response = match outcome {
        ReceiptOutcome::Pending => json!({ "status": "pending" }),
        ReceiptOutcome::Reverted => json!({ "status": "reverted" }),
        ReceiptOutcome::Confirmed => {
            match sum_erc20_transfers_to(&logs, contracts.mana_token, pay_to) {
                Some((from, total)) => json!({
                    "status": "confirmed",
                    "from": format!("{from:#x}"),
                    "to": format!("{pay_to:#x}"),
                    "valueWei": total.to_string(),
                }),
                None => json!({
                    "status": "no_payment",
                    "to": format!("{pay_to:#x}"),
                    "valueWei": "0",
                }),
            }
        }
    };
    tracing::info!(
        tx_hash = %tx_hash,
        outcome = ?outcome,
        "payment verification served"
    );
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, keccak256, Bytes};

    const MANA: Address = address!("0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4");
    const PAY_TO: Address = address!("0x1111111111111111111111111111111111111111");
    const SHOPPER: Address = address!("0x2222222222222222222222222222222222222222");
    const OTHER: Address = address!("0x3333333333333333333333333333333333333333");

    fn value_data(v: u64) -> Bytes {
        Bytes::from(U256::from(v).to_be_bytes::<32>().to_vec())
    }

    fn erc20_transfer(token: Address, from: Address, to: Address, value: u64) -> ReceiptLog {
        ReceiptLog {
            address: token,
            topics: vec![
                B256::from(ERC20_TRANSFER_TOPIC0),
                address_topic(from),
                address_topic(to),
            ],
            data: value_data(value),
        }
    }

    #[test]
    fn erc20_transfer_topic0_matches_keccak() {
        let computed = keccak256("Transfer(address,address,uint256)".as_bytes());
        assert_eq!(computed.as_slice(), &ERC20_TRANSFER_TOPIC0);
    }

    #[test]
    fn decodes_a_single_mana_transfer_to_pay_to() {
        let logs = vec![erc20_transfer(MANA, SHOPPER, PAY_TO, 5_000)];
        let (from, total) = sum_erc20_transfers_to(&logs, MANA, PAY_TO).expect("match");
        assert_eq!(from, SHOPPER);
        assert_eq!(total, U256::from(5_000u64));
    }

    #[test]
    fn sums_multiple_transfers_from_the_same_sender() {
        let logs = vec![
            erc20_transfer(MANA, SHOPPER, PAY_TO, 1_000),
            erc20_transfer(MANA, SHOPPER, PAY_TO, 2_500),
        ];
        let (from, total) = sum_erc20_transfers_to(&logs, MANA, PAY_TO).expect("match");
        assert_eq!(from, SHOPPER);
        assert_eq!(total, U256::from(3_500u64));
    }

    #[test]
    fn ignores_value_from_a_second_sender() {
        let logs = vec![
            erc20_transfer(MANA, SHOPPER, PAY_TO, 1_000),
            erc20_transfer(MANA, OTHER, PAY_TO, 9_999),
        ];
        let (from, total) = sum_erc20_transfers_to(&logs, MANA, PAY_TO).expect("match");
        assert_eq!(from, SHOPPER);
        assert_eq!(
            total,
            U256::from(1_000u64),
            "second sender must not inflate the first"
        );
    }

    #[test]
    fn ignores_non_matching_logs() {
        assert!(
            sum_erc20_transfers_to(&[erc20_transfer(OTHER, SHOPPER, PAY_TO, 1)], MANA, PAY_TO)
                .is_none()
        );
        assert!(
            sum_erc20_transfers_to(&[erc20_transfer(MANA, SHOPPER, OTHER, 1)], MANA, PAY_TO)
                .is_none()
        );
        let erc721 = ReceiptLog {
            address: MANA,
            topics: vec![
                B256::from(ERC20_TRANSFER_TOPIC0),
                address_topic(SHOPPER),
                address_topic(PAY_TO),
                B256::from(U256::from(7u64).to_be_bytes::<32>()),
            ],
            data: Bytes::new(),
        };
        assert!(sum_erc20_transfers_to(&[erc721], MANA, PAY_TO).is_none());
        let wrong_topic = ReceiptLog {
            address: MANA,
            topics: vec![B256::ZERO, address_topic(SHOPPER), address_topic(PAY_TO)],
            data: value_data(1),
        };
        assert!(sum_erc20_transfers_to(&[wrong_topic], MANA, PAY_TO).is_none());
        let short_data = ReceiptLog {
            address: MANA,
            topics: vec![
                B256::from(ERC20_TRANSFER_TOPIC0),
                address_topic(SHOPPER),
                address_topic(PAY_TO),
            ],
            data: Bytes::from(vec![0x01u8; 31]),
        };
        assert!(sum_erc20_transfers_to(&[short_data], MANA, PAY_TO).is_none());
    }

    #[test]
    fn config_json_contract() {
        let enabled = config_json(Some(PAY_TO), Some(MANA), 137);
        assert_eq!(
            enabled,
            serde_json::json!({
                "payTo": "0x1111111111111111111111111111111111111111",
                "manaToken": "0xa1c57f48f0deb89f569dfbe6e2b7f46d33606fd4",
                "chainId": 137,
                "enabled": true,
            })
        );
        let disabled = config_json(None, Some(MANA), 137);
        assert_eq!(disabled["enabled"], serde_json::json!(false));
        assert_eq!(disabled["payTo"], serde_json::Value::Null);
        let no_contracts = config_json(Some(PAY_TO), None, 5);
        assert_eq!(no_contracts["enabled"], serde_json::json!(false));
        assert_eq!(no_contracts["manaToken"], serde_json::Value::Null);
    }

    #[test]
    fn tx_hash_validation() {
        let ok = format!("0x{}", "Ab".repeat(32));
        assert_eq!(validate_tx_hash(&ok).unwrap(), ok.to_lowercase());
        assert!(validate_tx_hash("0x1234").is_err());
        assert!(validate_tx_hash(&format!("0x{}", "g".repeat(64))).is_err());
        assert!(validate_tx_hash(&"a".repeat(66)).is_err());
        assert!(validate_tx_hash("").is_err());
    }

    #[test]
    fn get_nonce_calldata_is_selector_plus_padded_address() {
        let data = encode_get_nonce(SHOPPER);
        assert_eq!(data.len(), 4 + 32);
        assert_eq!(&data[..4], &[0x2d, 0x03, 0x35, 0xab]);
        assert_eq!(&data[4..16], &[0u8; 12]);
        assert_eq!(&data[16..36], SHOPPER.as_slice());
    }

    #[test]
    fn get_nonce_return_decodes_uint256() {
        let ret = U256::from(42u64).to_be_bytes::<32>();
        assert_eq!(decode_get_nonce_return(&ret).unwrap(), U256::from(42u64));
        assert!(decode_get_nonce_return(&[0u8; 5]).is_err());
    }
}
