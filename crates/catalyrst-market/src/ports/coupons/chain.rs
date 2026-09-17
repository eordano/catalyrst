use alloy_primitives::{keccak256, Address, U256};
use async_trait::async_trait;
use futures::future::join_all;
use serde_json::json;
use std::str::FromStr;

use super::types::{CouponChainIndexes, CouponChainState};
use crate::ports::trades::RpcEndpoints;

#[derive(Debug)]
pub enum ChainReadError {
    NotConfigured(i64),
    Rpc(String),
}

impl std::fmt::Display for ChainReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainReadError::NotConfigured(chain_id) => write!(
                f,
                "the coupon manager cannot be read: no rpc endpoint is configured for chain {chain_id}"
            ),
            ChainReadError::Rpc(why) => write!(f, "coupon manager read failed: {why}"),
        }
    }
}

/// What the server asks a CouponManager on chain: whether it accepts a coupon contract, the
/// signature indexes at signing time, and a coupon's live state.
#[async_trait]
pub trait CouponChainReader: Send + Sync {
    async fn read_coupon_allowed(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        coupon: &str,
    ) -> Result<bool, ChainReadError>;

    async fn read_indexes(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        signer: &str,
    ) -> Result<CouponChainIndexes, ChainReadError>;

    async fn read_state(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        state_keys: &[String],
    ) -> Result<CouponChainState, ChainReadError>;
}

/// Reads the CouponManager over the per-chain RPC endpoints trades already use. `eth_call` by
/// hand rather than through a contract binding, matching `ports::trades::ownership`.
pub struct RpcCouponChainReader {
    http: reqwest::Client,
    endpoints: RpcEndpoints,
}

impl RpcCouponChainReader {
    pub fn new(http: reqwest::Client, endpoints: RpcEndpoints) -> Self {
        Self { http, endpoints }
    }

    async fn call(&self, chain_id: i64, to: &str, data: String) -> Result<String, ChainReadError> {
        let url = self
            .endpoints
            .for_chain(chain_id)
            .ok_or(ChainReadError::NotConfigured(chain_id))?;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{ "to": to, "data": data }, "latest"]
        });
        let response = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ChainReadError::Rpc(e.to_string()))?;
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ChainReadError::Rpc(format!("unreadable rpc response: {e}")))?;
        if let Some(error) = payload.get("error") {
            return Err(ChainReadError::Rpc(error.to_string()));
        }
        payload
            .get("result")
            .and_then(|r| r.as_str())
            .map(|r| r.to_string())
            .ok_or_else(|| ChainReadError::Rpc("rpc response carried no result".to_string()))
    }
}

/// First four bytes of keccak256 of the function signature.
fn selector(signature: &str) -> String {
    hex::encode(&keccak256(signature.as_bytes())[..4])
}

fn no_argument_calldata(signature: &str) -> String {
    format!("0x{}", selector(signature))
}

fn address_calldata(signature: &str, address: &str) -> Result<String, ChainReadError> {
    let parsed = Address::from_str(address)
        .map_err(|e| ChainReadError::Rpc(format!("bad address {address}: {e}")))?;
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(parsed.as_slice());
    Ok(format!("0x{}{}", selector(signature), hex::encode(word)))
}

fn bytes32_calldata(signature: &str, value: &str) -> Result<String, ChainReadError> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    let raw = hex::decode(trimmed)
        .map_err(|e| ChainReadError::Rpc(format!("bad bytes32 {value}: {e}")))?;
    if raw.len() != 32 {
        return Err(ChainReadError::Rpc(format!(
            "expected a 32-byte word, got {} bytes",
            raw.len()
        )));
    }
    Ok(format!("0x{}{}", selector(signature), hex::encode(raw)))
}

/// The manager answers uint256; these are narrowed to compare against the numbers the coupon was
/// signed with. Safe for what they are -- counters a wallet bumps one at a time.
fn word_to_u64(word: &str) -> Result<u64, ChainReadError> {
    let trimmed = word.strip_prefix("0x").unwrap_or(word);
    if trimmed.len() != 64 {
        return Err(ChainReadError::Rpc(format!(
            "the coupon manager returned {} hex chars, not one 32-byte word: {word}",
            trimmed.len()
        )));
    }
    let parsed = U256::from_str_radix(trimmed, 16)
        .map_err(|e| ChainReadError::Rpc(format!("undecodable uint word {word}: {e}")))?;
    Ok(parsed.saturating_to::<u64>())
}

fn word_to_bool(word: &str) -> Result<bool, ChainReadError> {
    Ok(word_to_u64(word)? != 0)
}

/// A coupon lives in exactly one manager's slot, so at most one of the slots read can hold
/// anything: the answer is whichever one does, not their sum.
fn fold_slots(slots: &[(i64, bool)]) -> CouponChainState {
    slots.iter().fold(
        CouponChainState {
            uses: 0,
            cancelled: false,
        },
        |state, (uses, cancelled)| CouponChainState {
            uses: state.uses.max(*uses),
            cancelled: state.cancelled || *cancelled,
        },
    )
}

const ALLOWED_COUPONS: &str = "allowedCoupons(address)";
const CONTRACT_SIGNATURE_INDEX: &str = "contractSignatureIndex()";
const SIGNER_SIGNATURE_INDEX: &str = "signerSignatureIndex(address)";
const SIGNATURE_USES: &str = "signatureUses(bytes32)";
const CANCELLED_SIGNATURES: &str = "cancelledSignatures(bytes32)";

#[async_trait]
impl CouponChainReader for RpcCouponChainReader {
    async fn read_coupon_allowed(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        coupon: &str,
    ) -> Result<bool, ChainReadError> {
        let word = self
            .call(
                chain_id,
                coupon_manager,
                address_calldata(ALLOWED_COUPONS, coupon)?,
            )
            .await?;
        word_to_bool(&word)
    }

    async fn read_indexes(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        signer: &str,
    ) -> Result<CouponChainIndexes, ChainReadError> {
        let contract_call = self.call(
            chain_id,
            coupon_manager,
            no_argument_calldata(CONTRACT_SIGNATURE_INDEX),
        );
        let signer_call = self.call(
            chain_id,
            coupon_manager,
            address_calldata(SIGNER_SIGNATURE_INDEX, signer)?,
        );
        let (contract_word, signer_word) = tokio::join!(contract_call, signer_call);
        Ok(CouponChainIndexes {
            contract_signature_index: word_to_u64(&contract_word?)?,
            signer_signature_index: word_to_u64(&signer_word?)?,
        })
    }

    /// A coupon can be keyed under more than one slot depending on the manager's generation, and
    /// only the one its own manager uses is ever written, so the answer is whichever slot holds
    /// something.
    async fn read_state(
        &self,
        chain_id: i64,
        coupon_manager: &str,
        state_keys: &[String],
    ) -> Result<CouponChainState, ChainReadError> {
        let mut calls = Vec::with_capacity(state_keys.len());
        for state_key in state_keys {
            calls.push((
                self.call(
                    chain_id,
                    coupon_manager,
                    bytes32_calldata(SIGNATURE_USES, state_key)?,
                ),
                self.call(
                    chain_id,
                    coupon_manager,
                    bytes32_calldata(CANCELLED_SIGNATURES, state_key)?,
                ),
            ));
        }
        let slots = join_all(
            calls
                .into_iter()
                .map(|(uses, cancelled)| async move { tokio::join!(uses, cancelled) }),
        )
        .await;
        let mut decoded = Vec::with_capacity(slots.len());
        for (uses_word, cancelled_word) in slots {
            decoded.push((
                word_to_u64(&uses_word?)? as i64,
                word_to_bool(&cancelled_word?)?,
            ));
        }
        Ok(fold_slots(&decoded))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the selector helper against the one function selector this crate already spells out
    /// by hand (ports::trades::ownership OWNER_OF_SELECTOR).
    #[test]
    fn the_selector_is_the_first_four_bytes_of_the_signature_hash() {
        assert_eq!(selector("ownerOf(uint256)"), "6352211e");
    }

    #[test]
    fn calldata_is_a_selector_plus_one_padded_word() {
        let data = address_calldata(
            SIGNER_SIGNATURE_INDEX,
            "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
        )
        .unwrap();
        assert_eq!(data.len(), 2 + 8 + 64);
        assert!(data.ends_with("abcdefabcdefabcdefabcdefabcdefabcdefabcd"));

        let key = format!("0x{}", "11".repeat(32));
        let state = bytes32_calldata(SIGNATURE_USES, &key).unwrap();
        assert_eq!(state.len(), 2 + 8 + 64);
        assert!(state.ends_with(&"11".repeat(32)));

        assert_eq!(no_argument_calldata(CONTRACT_SIGNATURE_INDEX).len(), 2 + 8);
    }

    #[test]
    fn a_bytes32_of_the_wrong_width_is_refused() {
        assert!(bytes32_calldata(SIGNATURE_USES, "0x11").is_err());
        assert!(bytes32_calldata(SIGNATURE_USES, "not hex").is_err());
    }

    #[test]
    fn the_slot_that_holds_something_answers_for_the_pair() {
        assert_eq!(
            fold_slots(&[(0, false), (3, false)]),
            CouponChainState {
                uses: 3,
                cancelled: false
            }
        );
        assert_eq!(
            fold_slots(&[(0, true), (0, false)]),
            CouponChainState {
                uses: 0,
                cancelled: true
            }
        );
        assert_eq!(
            fold_slots(&[(0, false), (0, false)]),
            CouponChainState {
                uses: 0,
                cancelled: false
            }
        );
        assert_eq!(
            fold_slots(&[]),
            CouponChainState {
                uses: 0,
                cancelled: false
            }
        );
    }

    #[test]
    fn the_allow_list_read_is_a_selector_over_the_coupon_address() {
        let data = address_calldata(
            ALLOWED_COUPONS,
            "0x4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9",
        )
        .unwrap();
        assert_eq!(data.len(), 2 + 8 + 64);
        assert!(data.ends_with("4ee8f6b87f4917a3bbc7c8bb3a06db8555f83db9"));
    }

    #[test]
    fn words_decode_as_counters_and_flags() {
        let zero = format!("0x{}", "00".repeat(32));
        let one = format!("0x{}{}", "00".repeat(31), "01");
        assert_eq!(word_to_u64(&zero).unwrap(), 0);
        assert_eq!(word_to_u64(&one).unwrap(), 1);
        assert!(!word_to_bool(&zero).unwrap());
        assert!(word_to_bool(&one).unwrap());
        assert!(
            word_to_u64("0x").is_err(),
            "empty returndata means no contract, not zero uses"
        );
        assert!(word_to_u64(&format!("0x{}", "00".repeat(31))).is_err());
    }
}
