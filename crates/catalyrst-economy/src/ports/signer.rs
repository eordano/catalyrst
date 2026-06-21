use std::str::FromStr;
use std::time::{Duration, Instant};

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, TxHash, B256, U256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::config::Config;
use crate::http::errors::ApiError;
use crate::ports::transaction::TransactionData;

pub struct DirectSigner {
    rpc_url: String,
    signer: PrivateKeySigner,
    chain_id: u64,
    relayer_address: Address,

    max_gas_limit: u64,

    receipt_poll_interval: Duration,
    receipt_timeout: Duration,

    send_lock: tokio::sync::Mutex<()>,

    provider: tokio::sync::OnceCell<DynProvider>,
}

fn within_gas_cap(estimate: u64, cap: u64) -> bool {
    cap == 0 || estimate <= cap
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptOutcome {
    Confirmed,
    Reverted,
    Pending,
}

pub fn classify_receipt(status: Option<bool>) -> ReceiptOutcome {
    match status {
        Some(true) => ReceiptOutcome::Confirmed,
        Some(false) => ReceiptOutcome::Reverted,
        None => ReceiptOutcome::Pending,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Bytes,
}

impl DirectSigner {
    pub fn from_config(cfg: &Config) -> Result<Option<Self>, String> {
        if !cfg.meta_tx_broadcast_enabled {
            return Ok(None);
        }
        let Some(key) = cfg.relayer_private_key.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let Some(rpc_url) = cfg.rpc_url.as_deref().filter(|s| !s.is_empty()) else {
            return Err(
                "META_TX_BROADCAST_ENABLED=true and RELAYER_PRIVATE_KEY set, but RPC_URL is empty"
                    .into(),
            );
        };
        Self::build(cfg, key, rpc_url, cfg.collections_chain_id).map(Some)
    }

    pub fn eth_from_config(cfg: &Config) -> Result<Option<Self>, String> {
        if !cfg.meta_tx_broadcast_enabled {
            return Ok(None);
        }
        let Some(key) = cfg.relayer_private_key.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let Some(rpc_url) = cfg.eth_rpc_url.as_deref().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        Self::build(cfg, key, rpc_url, cfg.names_chain_id).map(Some)
    }

    fn build(cfg: &Config, key: &str, rpc_url: &str, chain_id: u64) -> Result<Self, String> {
        let signer = PrivateKeySigner::from_str(key.trim_start_matches("0x"))
            .map_err(|e| format!("invalid RELAYER_PRIVATE_KEY: {e}"))?;
        let relayer_address = signer.address();

        Ok(Self {
            rpc_url: rpc_url.to_string(),
            signer,
            chain_id,
            relayer_address,
            max_gas_limit: cfg.max_gas_limit,
            receipt_poll_interval: Duration::from_millis(cfg.receipt_poll_interval_ms.max(1)),
            receipt_timeout: Duration::from_millis(cfg.receipt_timeout_ms),
            send_lock: tokio::sync::Mutex::new(()),
            provider: tokio::sync::OnceCell::new(),
        })
    }

    async fn provider(&self) -> Result<DynProvider, ApiError> {
        self.provider
            .get_or_try_init(|| async {
                ProviderBuilder::new()
                    .wallet(self.signer.clone())
                    .connect(&self.rpc_url)
                    .await
                    .map(Provider::erased)
                    .map_err(|e| {
                        ApiError::RelayerFailed(format!("could not connect to RPC node: {e}"))
                    })
            })
            .await
            .cloned()
    }

    pub fn relayer_address(&self) -> Address {
        self.relayer_address
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub async fn await_receipt(&self, tx_hash_hex: &str) -> Result<ReceiptOutcome, ApiError> {
        let (outcome, _logs) = self.await_receipt_detailed(tx_hash_hex).await?;
        Ok(outcome)
    }

    pub async fn await_receipt_detailed(
        &self,
        tx_hash_hex: &str,
    ) -> Result<(ReceiptOutcome, Vec<ReceiptLog>), ApiError> {
        let hash = parse_tx_hash(tx_hash_hex)?;

        let provider = self.provider().await?;

        let deadline = Instant::now() + self.receipt_timeout;
        loop {
            match provider.get_transaction_receipt(hash).await {
                Ok(Some(receipt)) => {
                    let outcome = classify_receipt(Some(receipt.status()));
                    let logs = receipt_logs(&receipt);
                    tracing::info!(
                        tx_hash = %tx_hash_hex,
                        outcome = ?outcome,
                        "broker tx receipt observed"
                    );
                    return Ok((outcome, logs));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(tx_hash = %tx_hash_hex, error = %e, "receipt poll RPC error; retrying");
                }
            }
            if Instant::now() >= deadline {
                tracing::warn!(
                    tx_hash = %tx_hash_hex,
                    timeout_ms = self.receipt_timeout.as_millis() as u64,
                    "broker tx not mined within receipt timeout; leaving for reconciler"
                );
                return Ok((ReceiptOutcome::Pending, Vec::new()));
            }
            tokio::time::sleep(self.receipt_poll_interval).await;
        }
    }

    pub async fn fetch_receipt_once(
        &self,
        tx_hash_hex: &str,
    ) -> Result<(ReceiptOutcome, Vec<ReceiptLog>), ApiError> {
        let hash = parse_tx_hash(tx_hash_hex)?;
        let provider = self.provider().await?;
        match provider.get_transaction_receipt(hash).await {
            Ok(Some(receipt)) => Ok((
                classify_receipt(Some(receipt.status())),
                receipt_logs(&receipt),
            )),
            Ok(None) => Ok((ReceiptOutcome::Pending, Vec::new())),
            Err(e) => Err(ApiError::RelayerFailed(format!(
                "receipt lookup failed for {tx_hash_hex}: {e}"
            ))),
        }
    }

    pub async fn eth_call(&self, to: Address, data: Bytes) -> Result<Bytes, ApiError> {
        let provider = self.provider().await?;
        let request = TransactionRequest::default().with_to(to).with_input(data);
        provider
            .call(request)
            .await
            .map_err(|e| ApiError::RelayerFailed(format!("eth_call failed: {e}")))
    }

    pub async fn send_meta_transaction(&self, tx: &TransactionData) -> Result<String, ApiError> {
        let to = Address::from_str(tx.params[0].trim())
            .map_err(|e| ApiError::InvalidTransaction(format!("invalid `to` address: {e}")))?;
        let data = Bytes::from_str(tx.params[1].trim())
            .map_err(|e| ApiError::InvalidTransaction(format!("invalid call data: {e}")))?;

        self.broadcast(to, data, "broadcast meta-transaction via direct JSON-RPC")
            .await
    }

    pub async fn send_direct_call(&self, to: Address, data: Bytes) -> Result<String, ApiError> {
        self.broadcast(to, data, "broadcast direct contract call via JSON-RPC")
            .await
    }

    async fn broadcast(
        &self,
        to: Address,
        data: Bytes,
        log_msg: &'static str,
    ) -> Result<String, ApiError> {
        let _guard = self.send_lock.lock().await;

        let provider = self.provider().await?;

        let mut request = TransactionRequest::default()
            .with_to(to)
            .with_input(data)
            .with_value(U256::ZERO)
            .with_chain_id(self.chain_id)
            .with_from(self.relayer_address);

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

        request = request.with_gas_limit(estimate);

        let pending = provider.send_transaction(request).await.map_err(|e| {
            let msg = e.to_string();

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
            "{log_msg}"
        );
        Ok(format!("{tx_hash:#x}"))
    }
}

fn parse_tx_hash(tx_hash_hex: &str) -> Result<TxHash, ApiError> {
    TxHash::from_str(tx_hash_hex.trim()).map_err(|e| {
        ApiError::Internal(format!(
            "invalid tx hash {tx_hash_hex:?} for receipt poll: {e}"
        ))
    })
}

fn receipt_logs(receipt: &alloy::rpc::types::TransactionReceipt) -> Vec<ReceiptLog> {
    receipt
        .logs()
        .iter()
        .map(|l| ReceiptLog {
            address: l.address(),
            topics: l.topics().to_vec(),
            data: l.data().data.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{classify_receipt, within_gas_cap, ReceiptOutcome};

    #[test]
    fn receipt_status_maps_to_outcome() {
        assert_eq!(classify_receipt(Some(true)), ReceiptOutcome::Confirmed);
        assert_eq!(classify_receipt(Some(false)), ReceiptOutcome::Reverted);
        assert_eq!(classify_receipt(None), ReceiptOutcome::Pending);
    }

    #[test]
    fn gas_cap_enforced() {
        assert!(within_gas_cap(250_000, 1_500_000));
        assert!(within_gas_cap(1_500_000, 1_500_000));
        assert!(!within_gas_cap(1_500_001, 1_500_000));
        assert!(!within_gas_cap(30_000_000, 1_500_000));

        assert!(within_gas_cap(30_000_000, 0));
    }
}
