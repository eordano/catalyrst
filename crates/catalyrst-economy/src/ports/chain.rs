use std::time::Duration;

use alloy::primitives::{Address, B256, U256};
use alloy::sol;
use alloy::sol_types::SolCall;
use catalyrst_commons::cache::{TtlCell, TtlMap};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::ports::abi::getNonceCall;

sol! {
    function domainSeparator() external view returns (bytes32);
    function getDomainSeperator() external view returns (bytes32);
}

const DEFAULT_RPC_TIMEOUT_MS: u64 = 10_000;

/// A contract's domain separator and the node's chain id never change, so they
/// are memoized for a day (bounded only so a redeploy cannot serve them forever).
const IMMUTABLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const DOMAIN_SEPARATOR_CACHE_CAP: usize = 4096;

#[derive(Debug)]
pub enum ChainError {
    Transport(String),
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainError::Transport(m) => write!(f, "rpc unavailable: {m}"),
        }
    }
}

pub struct ChainComponent {
    http: reqwest::Client,
    rpc_url: String,
    timeout: Duration,
    domain_separators: TtlMap<Address, Option<B256>>,
    chain_id: TtlCell<u64>,
}

impl ChainComponent {
    pub fn from_config(cfg: &Config) -> Option<Self> {
        let url = cfg.rpc_url.as_deref().filter(|s| !s.is_empty())?;
        Some(Self::new(
            url,
            Duration::from_millis(DEFAULT_RPC_TIMEOUT_MS),
        ))
    }

    pub fn new(rpc_url: &str, timeout: Duration) -> Self {
        Self {
            http: reqwest::Client::new(),
            rpc_url: rpc_url.to_string(),
            timeout,
            domain_separators: TtlMap::bounded(
                "economy-domain-separator",
                IMMUTABLE_TTL,
                DOMAIN_SEPARATOR_CACHE_CAP,
            ),
            chain_id: TtlCell::new("economy-chain-id"),
        }
    }

    async fn rpc(&self, body: Value) -> Result<JsonRpcResponse, ChainError> {
        let resp = self
            .http
            .post(&self.rpc_url)
            .timeout(self.timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| ChainError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ChainError::Transport(format!(
                "rpc responded with status {}",
                resp.status().as_u16()
            )));
        }
        resp.json::<JsonRpcResponse>()
            .await
            .map_err(|e| ChainError::Transport(e.to_string()))
    }

    /// An `eth_call`, distinguishing a target that reverted or returned nothing
    /// (`Ok(None)`) from an RPC that could not answer (`Err`). Only a revert
    /// describes the target; any transport-level failure describes the node and
    /// must never be read as a property of the contract.
    async fn call_view(&self, to: Address, data: Vec<u8>) -> Result<Option<Vec<u8>>, ChainError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{
                "to": format!("{to:#x}"),
                "data": format!("0x{}", alloy::hex::encode(&data)),
            }, "latest"],
        });
        let resp = self.rpc(body).await?;
        if let Some(err) = resp.error {
            if is_execution_revert(&err) {
                return Ok(None);
            }
            return Err(ChainError::Transport(format!(
                "eth_call failed ({}): {}",
                err.code, err.message
            )));
        }
        let hex = resp
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChainError::Transport("eth_call returned no result".into()))?;
        let bytes = decode_hex(hex)
            .ok_or_else(|| ChainError::Transport(format!("eth_call returned non-hex: {hex}")))?;
        Ok(Some(bytes))
    }

    pub async fn get_meta_transaction_nonce(
        &self,
        contract: Address,
        user: Address,
    ) -> Result<Option<U256>, ChainError> {
        let data = getNonceCall { user }.abi_encode();
        let Some(result) = self.call_view(contract, data).await? else {
            return Ok(None);
        };
        match getNonceCall::abi_decode_returns(&result) {
            Ok(nonce) => Ok(Some(nonce)),
            Err(_) => Ok(None),
        }
    }

    pub async fn get_domain_separator(
        &self,
        contract: Address,
    ) -> Result<Option<B256>, ChainError> {
        self.domain_separators
            .get_or_fetch(contract, || self.fetch_domain_separator(contract))
            .await
    }

    async fn fetch_domain_separator(&self, contract: Address) -> Result<Option<B256>, ChainError> {
        if let Some(result) = self
            .call_view(contract, domainSeparatorCall {}.abi_encode())
            .await?
        {
            if let Ok(sep) = domainSeparatorCall::abi_decode_returns(&result) {
                return Ok(Some(sep));
            }
        }
        if let Some(result) = self
            .call_view(contract, getDomainSeperatorCall {}.abi_encode())
            .await?
        {
            if let Ok(sep) = getDomainSeperatorCall::abi_decode_returns(&result) {
                return Ok(Some(sep));
            }
        }
        Ok(None)
    }

    pub async fn get_chain_id(&self) -> Result<u64, ChainError> {
        self.chain_id
            .get_or_refresh(IMMUTABLE_TTL, || self.fetch_chain_id())
            .await
    }

    async fn fetch_chain_id(&self) -> Result<u64, ChainError> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": [] });
        let resp = self.rpc(body).await?;
        if let Some(err) = resp.error {
            return Err(ChainError::Transport(format!(
                "eth_chainId failed ({}): {}",
                err.code, err.message
            )));
        }
        let hex = resp
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChainError::Transport("eth_chainId returned no result".into()))?;
        u64::from_str_radix(hex.trim_start_matches("0x"), 16)
            .map_err(|e| ChainError::Transport(format!("bad chain id {hex}: {e}")))
    }

    pub async fn get_code(&self, address: Address) -> Result<Vec<u8>, ChainError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getCode",
            "params": [format!("{address:#x}"), "latest"],
        });
        let resp = self.rpc(body).await?;
        if let Some(err) = resp.error {
            return Err(ChainError::Transport(format!(
                "eth_getCode failed ({}): {}",
                err.code, err.message
            )));
        }
        let hex = resp
            .result
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChainError::Transport("eth_getCode returned no result".into()))?;
        decode_hex(hex).ok_or_else(|| ChainError::Transport(format!("eth_getCode non-hex: {hex}")))
    }

    pub async fn is_contract(&self, address: Address) -> Result<bool, ChainError> {
        Ok(!self.get_code(address).await?.is_empty())
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let stripped = hex.strip_prefix("0x").unwrap_or(hex);
    if stripped.is_empty() {
        return Some(Vec::new());
    }
    alloy::hex::decode(stripped).ok()
}

/// Whether a JSON-RPC error is the target reverting rather than the node
/// failing. Reverts arrive as EIP-1474 code 3 or as node-specific server errors
/// whose message names the revert; anything else is a transport failure that
/// must propagate so an RPC hiccup is never mistaken for a rejecting contract.
fn is_execution_revert(err: &JsonRpcError) -> bool {
    if err.code == 3 {
        return true;
    }
    let m = err.message.to_ascii_lowercase();
    m.contains("execution reverted") || m.contains("revert")
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_is_distinguished_from_transport() {
        assert!(is_execution_revert(&JsonRpcError {
            code: 3,
            message: "execution reverted".into()
        }));
        assert!(is_execution_revert(&JsonRpcError {
            code: -32000,
            message: "execution reverted: bad".into()
        }));
        assert!(!is_execution_revert(&JsonRpcError {
            code: -32000,
            message: "header not found".into()
        }));
        assert!(!is_execution_revert(&JsonRpcError {
            code: 429,
            message: "rate limited".into()
        }));
    }

    #[test]
    fn empty_code_decodes_to_no_bytes() {
        assert_eq!(decode_hex("0x"), Some(Vec::new()));
        assert_eq!(decode_hex(""), Some(Vec::new()));
        assert_eq!(decode_hex("0x0102"), Some(vec![0x01, 0x02]));
        assert_eq!(decode_hex("0xzz"), None);
    }

    async fn spawn_counting_rpc() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = axum::Router::new().route(
            "/",
            axum::routing::post(move |axum::Json(req): axum::Json<Value>| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let id = req.get("id").cloned().unwrap_or(json!(1));
                    let result = match req["method"].as_str() {
                        Some("eth_chainId") => json!("0x89"),
                        Some("eth_call") => {
                            let data = req["params"][0]["data"].as_str().unwrap_or("");
                            if data.starts_with(&format!(
                                "0x{}",
                                alloy::hex::encode(domainSeparatorCall::SELECTOR)
                            )) {
                                json!(format!("0x{}", "5a".repeat(32)))
                            } else {
                                return axum::Json(json!({
                                    "jsonrpc": "2.0", "id": id,
                                    "error": { "code": 3, "message": "execution reverted" }
                                }));
                            }
                        }
                        _ => json!(null),
                    };
                    axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), calls)
    }

    #[tokio::test]
    async fn domain_separator_and_chain_id_are_fetched_once_per_contract() {
        use std::sync::atomic::Ordering;

        let (url, calls) = spawn_counting_rpc().await;
        let chain = ChainComponent::new(&url, Duration::from_secs(5));
        let contract = Address::repeat_byte(0x11);

        let first = chain.get_domain_separator(contract).await.unwrap();
        let second = chain.get_domain_separator(contract).await.unwrap();
        assert_eq!(first, Some(B256::repeat_byte(0x5a)));
        assert_eq!(first, second);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second lookup is served from memory"
        );

        assert_eq!(chain.get_chain_id().await.unwrap(), 137);
        assert_eq!(chain.get_chain_id().await.unwrap(), 137);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let other = Address::repeat_byte(0x22);
        assert_eq!(chain.get_domain_separator(other).await.unwrap(), first);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "each contract is fetched once"
        );
    }
}
