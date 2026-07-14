use alloy_primitives::{Address, U256};
use serde_json::json;
use std::str::FromStr;

const OWNER_OF_SELECTOR: &str = "6352211e";

#[derive(Debug)]
pub enum OwnershipError {
    NotConfigured(i64),
    Rpc(String),
    NoOwner(String),
}

impl std::fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OwnershipError::NotConfigured(chain) => write!(
                f,
                "ownership cannot be verified: no rpc endpoint is configured for chain {chain}"
            ),
            OwnershipError::Rpc(why) => write!(f, "ownership lookup failed: {why}"),
            OwnershipError::NoOwner(token) => {
                write!(f, "token {token} has no owner on chain")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RpcEndpoints {
    endpoints: Vec<(i64, String)>,
}

impl RpcEndpoints {
    // TRADE_RPC_URLS is "<chainId>=<url>" pairs, comma separated, so a deployment
    // can verify ownership on exactly the chains it trusts and no others.
    pub fn from_env(raw: Option<&str>) -> Self {
        let endpoints = raw
            .unwrap_or("")
            .split(',')
            .filter_map(|entry| {
                let (chain, url) = entry.split_once('=')?;
                let chain = chain.trim().parse::<i64>().ok()?;
                let url = url.trim();
                if url.is_empty() {
                    return None;
                }
                Some((chain, url.to_string()))
            })
            .collect();
        RpcEndpoints { endpoints }
    }

    pub fn for_chain(&self, chain_id: i64) -> Option<&str> {
        self.endpoints
            .iter()
            .find(|(chain, _)| *chain == chain_id)
            .map(|(_, url)| url.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
    }
}

fn owner_of_calldata(token_id: &str) -> Result<String, OwnershipError> {
    let parsed = U256::from_str(token_id)
        .map_err(|e| OwnershipError::Rpc(format!("bad token id {token_id}: {e}")))?;
    Ok(format!(
        "0x{OWNER_OF_SELECTOR}{}",
        hex::encode(parsed.to_be_bytes::<32>())
    ))
}

fn address_from_word(word: &str) -> Result<Address, OwnershipError> {
    let trimmed = word.strip_prefix("0x").unwrap_or(word);
    if trimmed.len() < 40 {
        return Err(OwnershipError::NoOwner(word.to_string()));
    }
    let tail = &trimmed[trimmed.len() - 40..];
    Address::from_str(&format!("0x{tail}"))
        .map_err(|e| OwnershipError::Rpc(format!("undecodable owner word {word}: {e}")))
}

pub async fn owner_of(
    http: &reqwest::Client,
    endpoints: &RpcEndpoints,
    chain_id: i64,
    contract_address: &str,
    token_id: &str,
) -> Result<Address, OwnershipError> {
    let url = endpoints
        .for_chain(chain_id)
        .ok_or(OwnershipError::NotConfigured(chain_id))?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": contract_address, "data": owner_of_calldata(token_id)? },
            "latest"
        ]
    });
    let response = http
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| OwnershipError::Rpc(e.to_string()))?;
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|e| OwnershipError::Rpc(format!("unreadable rpc response: {e}")))?;
    if let Some(error) = payload.get("error") {
        return Err(OwnershipError::Rpc(error.to_string()));
    }
    let result = payload
        .get("result")
        .and_then(|r| r.as_str())
        .ok_or_else(|| OwnershipError::Rpc("rpc response carried no result".to_string()))?;
    let owner = address_from_word(result)?;
    if owner == Address::ZERO {
        return Err(OwnershipError::NoOwner(token_id.to_string()));
    }
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_parse_per_chain() {
        let e = RpcEndpoints::from_env(Some("137=https://polygon.example, 1=https://eth.example"));
        assert_eq!(e.for_chain(137), Some("https://polygon.example"));
        assert_eq!(e.for_chain(1), Some("https://eth.example"));
        assert_eq!(e.for_chain(42), None);
    }

    #[test]
    fn an_unset_variable_configures_nothing() {
        assert!(RpcEndpoints::from_env(None).is_empty());
        assert!(RpcEndpoints::from_env(Some("")).is_empty());
        assert!(RpcEndpoints::from_env(Some("137=")).is_empty());
    }

    #[test]
    fn owner_of_encodes_the_token_id_as_a_full_word() {
        let data = owner_of_calldata("42").unwrap();
        assert!(data.starts_with("0x6352211e"));
        assert_eq!(data.len(), 2 + 8 + 64);
        assert!(data.ends_with("2a"));
    }

    #[test]
    fn the_owner_is_the_low_twenty_bytes_of_the_word() {
        let word = "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let owner = address_from_word(word).unwrap();
        assert_eq!(
            owner,
            Address::from_str("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap()
        );
    }

    #[test]
    fn a_truncated_word_is_not_an_owner() {
        assert!(matches!(
            address_from_word("0x00"),
            Err(OwnershipError::NoOwner(_))
        ));
    }
}
