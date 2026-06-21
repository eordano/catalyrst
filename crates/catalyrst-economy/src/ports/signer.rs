//! Direct EVM JSON-RPC broadcast path for validated meta-transactions.
//!
//! Upstream `transactions-server` always broadcasts through a third-party
//! relayer (OpenZeppelin Defender or Gelato — see
//! `github.com-decentraland/transactions-server/src/ports/{openzeppelin,gelato}`).
//! Both providers take the already-encoded forwarder call (`to = params[0]`,
//! `data = params[1]`) and submit it as an ordinary transaction signed by the
//! provider's own funded relayer account, paying the gas. The meta-tx itself
//! is the user's EIP-712 signature already baked into `data`; the on-chain
//! `from` is the relayer, never the user.
//!
//! This module reproduces that final step ourselves, without a SaaS relayer:
//! it signs a normal EVM transaction with a locally-configured relayer private
//! key and submits it via `eth_sendRawTransaction` to a JSON-RPC node. It is a
//! drop-in alternative to [`crate::ports::relayer::Relayer`] (the OZ HTTP
//! client) for operators who run their own funded key + RPC endpoint.
//!
//! Safety: this path is OFF unless `META_TX_BROADCAST_ENABLED=true` AND a
//! `RELAYER_PRIVATE_KEY` is present. With either unset, broadcast falls through
//! to the existing 503 ("validation passed, broadcast unavailable") behaviour.

use std::str::FromStr;

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::config::Config;
use crate::http::errors::ApiError;
use crate::ports::transaction::TransactionData;

/// Self-hosted broadcaster: signs the forwarder call with a configured relayer
/// key and submits it over JSON-RPC. Construction validates the key + RPC URL
/// eagerly so a misconfigured deployment fails fast at startup.
pub struct DirectSigner {
    rpc_url: String,
    signer: PrivateKeySigner,
    chain_id: u64,
    relayer_address: Address,
    /// Hard cap on the gas limit we will broadcast (protects a funded relayer
    /// key from a valid-but-expensive call). 0 = uncapped.
    max_gas_limit: u64,
    /// Serializes broadcasts so concurrent requests can't be assigned the same
    /// nonce. Each call fetches the pending nonce, builds, and submits while
    /// holding the lock, so the next call sees the updated pending count.
    send_lock: tokio::sync::Mutex<()>,
}

/// True iff a broadcast with `estimate` gas units is within `cap` (0 = uncapped).
fn within_gas_cap(estimate: u64, cap: u64) -> bool {
    cap == 0 || estimate <= cap
}

impl DirectSigner {
    /// Build from config when, and only when, the self-broadcast path is both
    /// enabled and fully configured. Returns:
    ///   - `Ok(None)`        when disabled / key absent (safe default).
    ///   - `Ok(Some(_))`     when enabled and the key + RPC parse cleanly.
    ///   - `Err(_)`          when enabled but misconfigured (bad key / no RPC).
    pub fn from_config(cfg: &Config) -> Result<Option<Self>, String> {
        if !cfg.meta_tx_broadcast_enabled {
            return Ok(None);
        }
        let Some(key) = cfg.relayer_private_key.as_deref().filter(|s| !s.is_empty()) else {
            // Enabled but no key: treat as "not provisioned" rather than a hard
            // failure, so the 503 fallthrough still applies.
            return Ok(None);
        };
        let Some(rpc_url) = cfg.rpc_url.as_deref().filter(|s| !s.is_empty()) else {
            return Err(
                "META_TX_BROADCAST_ENABLED=true and RELAYER_PRIVATE_KEY set, but RPC_URL is empty"
                    .into(),
            );
        };

        let signer = PrivateKeySigner::from_str(key.trim_start_matches("0x"))
            .map_err(|e| format!("invalid RELAYER_PRIVATE_KEY: {e}"))?;
        let relayer_address = signer.address();

        Ok(Some(Self {
            rpc_url: rpc_url.to_string(),
            signer,
            chain_id: cfg.collections_chain_id,
            relayer_address,
            max_gas_limit: cfg.max_gas_limit,
            send_lock: tokio::sync::Mutex::new(()),
        }))
    }

    /// The on-chain account that will appear as `from` / pays gas. Logged at
    /// startup so operators can fund / monitor it.
    pub fn relayer_address(&self) -> Address {
        self.relayer_address
    }

    /// Sign and broadcast the validated forwarder call, returning the tx hash
    /// in the same `0x…` hex shape as the upstream relayer paths.
    ///
    /// Gas policy: fees/EIP-1559 are delegated to the node + alloy's filler
    /// stack, but the gas LIMIT is explicitly capped (`max_gas_limit`) so a
    /// valid-but-expensive allowlisted call can't drain the funded relayer key —
    /// the validation phase only caps gas PRICE. Broadcasts are serialized via
    /// `send_lock` so concurrent requests can't collide on the relayer nonce.
    pub async fn send_meta_transaction(&self, tx: &TransactionData) -> Result<String, ApiError> {
        let to = Address::from_str(tx.params[0].trim())
            .map_err(|e| ApiError::InvalidTransaction(format!("invalid `to` address: {e}")))?;
        let data = Bytes::from_str(tx.params[1].trim())
            .map_err(|e| ApiError::InvalidTransaction(format!("invalid call data: {e}")))?;

        // Serialize: hold the lock across estimate -> nonce -> submit so two
        // concurrent broadcasts can't be assigned the same pending nonce.
        let _guard = self.send_lock.lock().await;

        // Wallet-filling provider: signs locally, fills nonce/gas/fees, then
        // submits via eth_sendRawTransaction.
        let provider = ProviderBuilder::new()
            .wallet(self.signer.clone())
            .connect(&self.rpc_url)
            .await
            .map_err(|e| ApiError::RelayerFailed(format!("could not connect to RPC node: {e}")))?;

        let mut request = TransactionRequest::default()
            .with_to(to)
            .with_input(data)
            .with_value(U256::ZERO)
            .with_chain_id(self.chain_id)
            .with_from(self.relayer_address);

        // Estimate gas and enforce the hard cap BEFORE signing/broadcasting.
        let estimate = provider.estimate_gas(request.clone()).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("revert") || msg.contains("execution reverted") {
                ApiError::RelayReverted(format!("gas estimation reverted: {msg}"))
            } else {
                ApiError::RelayerFailed(format!("eth_estimateGas failed: {msg}"))
            }
        })?;
        if !within_gas_cap(estimate, self.max_gas_limit) {
            return Err(ApiError::InvalidTransaction(format!(
                "estimated gas {estimate} exceeds the relayer cap {} (raise MAX_GAS_LIMIT if intentional)",
                self.max_gas_limit
            )));
        }
        // Pin the limit to the estimate so the node can't bill more than checked.
        request = request.with_gas_limit(estimate);

        let pending = provider.send_transaction(request).await.map_err(|e| {
            let msg = e.to_string();
            // Node-side rejections (revert on estimate, bad nonce, etc.) are
            // the caller's problem (400); transport failures are ours (500).
            if msg.contains("revert") || msg.contains("execution reverted") {
                ApiError::RelayReverted(format!("transaction reverted: {msg}"))
            } else {
                ApiError::RelayerFailed(format!("eth_sendRawTransaction failed: {msg}"))
            }
        })?;

        let tx_hash = *pending.tx_hash();
        tracing::info!(
            tx_hash = %tx_hash,
            relayer = %self.relayer_address,
            "broadcast meta-transaction via direct JSON-RPC"
        );
        Ok(format!("{tx_hash:#x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::within_gas_cap;

    #[test]
    fn gas_cap_enforced() {
        // under / at the cap pass; over the cap is rejected
        assert!(within_gas_cap(250_000, 1_500_000));
        assert!(within_gas_cap(1_500_000, 1_500_000));
        assert!(!within_gas_cap(1_500_001, 1_500_000));
        assert!(!within_gas_cap(30_000_000, 1_500_000));
        // 0 = uncapped (explicit opt-out)
        assert!(within_gas_cap(30_000_000, 0));
    }
}
