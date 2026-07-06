use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

const SCORE_URL: &str = "https://score.snapshot.org/";

pub const MIN_USER_ACTIVITY: f64 = 100.0;

fn strategies() -> Value {
    json!([
        {
            "name": "multichain",
            "network": "1",
            "params": {
                "name": "multichain",
                "graphs": { "137": "subgraph.decentraland.org/blocks-matic-mainnet" },
                "symbol": "MANA",
                "strategies": [
                    {
                        "name": "erc20-balance-of",
                        "params": { "address": "0x0f5d2fb29fb7d3cfee444a200298f468908cc942", "decimals": 18 },
                        "network": "1"
                    },
                    {
                        "name": "erc20-balance-of",
                        "params": { "address": "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4", "decimals": 18 },
                        "network": "137"
                    }
                ]
            }
        },
        {
            "name": "erc20-balance-of",
            "network": "1",
            "params": { "symbol": "WMANA", "address": "0xfd09cf7cfffa9932e33668311c4777cb9db3c9be", "decimals": 18 }
        },
        {
            "name": "erc721-with-multiplier",
            "network": "1",
            "params": { "symbol": "LAND", "address": "0xf87e31492faf9a91b02ee0deaad50d51d56d5d4d", "multiplier": 2000 }
        },
        {
            "name": "decentraland-estate-size",
            "network": "1",
            "params": { "symbol": "ESTATE", "address": "0x959e104e1a4db6317fa58f8295f586e1a978c297", "multiplier": 2000 }
        },
        {
            "name": "erc721-with-multiplier",
            "network": "1",
            "params": { "symbol": "NAMES", "address": "0x2a187453064356c898cae034eaed119e1663acb8", "multiplier": 100 }
        }
    ])
}

fn is_ethereum_address(address: &str) -> bool {
    let bytes = address.as_bytes();
    bytes.len() == 42
        && address.starts_with("0x")
        && address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build snapshot reqwest client")
    })
}

pub async fn fetch_score(address: &str) -> f64 {
    if !is_ethereum_address(address) {
        return 0.0;
    }

    let payload = json!({
        "jsonrpc": "2.0",
        "method": "get_vp",
        "params": {
            "network": "1",
            "address": address.to_lowercase(),
            "strategies": strategies(),
            "space": "snapshot.dcl.eth",
            "delegation": false
        }
    });

    let result = async {
        let res = client()
            .post(SCORE_URL)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;
        let body: Value = res.json().await?;
        Ok::<f64, reqwest::Error>(
            body.get("result")
                .and_then(|r| r.get("vp"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        )
    }
    .await;

    match result {
        Ok(vp) => to_int32(vp) as f64,
        Err(err) => {
            tracing::error!(error = %err, address = %address, "Error loading user score");
            0.0
        }
    }
}

fn to_int32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc();
    let modulo = truncated.rem_euclid(4_294_967_296.0);
    if modulo >= 2_147_483_648.0 {
        (modulo - 4_294_967_296.0) as i32
    } else {
        modulo as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eth_address_validation() {
        assert!(is_ethereum_address(
            "0x0f5d2fb29fb7d3cfee444a200298f468908cc942"
        ));
        assert!(is_ethereum_address(
            "0xA1c57f48F0Deb89f569dFbE6E2B7f46D33606fD4"
        ));
        assert!(!is_ethereum_address("0x123"));
        assert!(!is_ethereum_address("not-an-address"));
        assert!(!is_ethereum_address(""));
        assert!(!is_ethereum_address(
            "0x0f5d2fb29fb7d3cfee444a200298f468908cc94z"
        ));
    }

    #[test]
    fn int32_truncation_matches_js() {
        assert_eq!(to_int32(123.9), 123);
        assert_eq!(to_int32(0.0), 0);
        assert_eq!(to_int32(100.0), 100);

        assert_eq!(to_int32(4_294_967_297.0), 1);
    }
}
