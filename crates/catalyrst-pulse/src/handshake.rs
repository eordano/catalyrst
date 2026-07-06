use std::collections::BTreeMap;

use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::{AuthChain, AuthLink, AuthLinkType};

use crate::decentraland::pulse::HandshakeRequest;

pub const MAX_TIMESTAMP_SKEW_MS: i64 = 60_000;

const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
const TIMESTAMP_HEADER: &str = "x-identity-timestamp";
const METADATA_HEADER: &str = "x-identity-metadata";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    InvalidJson,

    NoAuthChain,

    StaleTimestamp,

    InvalidAuthChain(String),
}

impl HandshakeError {
    pub fn message(&self) -> String {
        match self {
            HandshakeError::InvalidJson => "Invalid auth chain JSON".to_string(),
            HandshakeError::NoAuthChain => "No x-identity-auth-chain-* headers found.".to_string(),
            HandshakeError::StaleTimestamp => {
                format!("timestamp outside ±{MAX_TIMESTAMP_SKEW_MS}ms skew window")
            }
            HandshakeError::InvalidAuthChain(e) => e.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedHandshake {
    pub user_address: String,

    pub timestamp: String,
}

pub fn build_signed_fetch_payload(
    method: &str,
    path: &str,
    timestamp: &str,
    metadata: &str,
) -> String {
    format!(
        "{}:{}:{}:{}",
        method.to_lowercase(),
        path.to_lowercase(),
        timestamp,
        metadata
    )
}

fn parse_header_bag(
    headers: &BTreeMap<String, String>,
) -> Result<(AuthChain, String, String), HandshakeError> {
    let mut indexed: Vec<(u32, AuthLink)> = Vec::new();
    let mut timestamp = String::new();
    let mut metadata = String::new();

    for (key, value) in headers {
        let lk = key.to_lowercase();
        if let Some(suffix) = lk.strip_prefix(AUTH_CHAIN_HEADER_PREFIX) {
            if let Ok(idx) = suffix.parse::<u32>() {
                let link: AuthLink =
                    serde_json::from_str(value).map_err(|_| HandshakeError::InvalidJson)?;
                indexed.push((idx, link));
            }
        } else if lk == TIMESTAMP_HEADER {
            timestamp = value.clone();
        } else if lk == METADATA_HEADER {
            metadata = value.clone();
        }
    }

    if indexed.is_empty() {
        return Err(HandshakeError::NoAuthChain);
    }
    indexed.sort_by_key(|(i, _)| *i);
    let chain: AuthChain = indexed.into_iter().map(|(_, l)| l).collect();
    Ok((chain, timestamp, metadata))
}

fn signer_address(chain: &AuthChain) -> Option<String> {
    chain
        .first()
        .filter(|l| l.link_type == AuthLinkType::SIGNER)
        .map(|l| l.payload.trim().to_lowercase())
}

pub fn verify_handshake(
    request: &HandshakeRequest,
    now_ms: i64,
) -> Result<VerifiedHandshake, HandshakeError> {
    let json = std::str::from_utf8(&request.auth_chain).map_err(|_| HandshakeError::InvalidJson)?;
    let headers: BTreeMap<String, String> =
        serde_json::from_str(json).map_err(|_| HandshakeError::InvalidJson)?;

    let (chain, timestamp, metadata) = parse_header_bag(&headers)?;

    let ts_ms: i64 = timestamp
        .parse()
        .map_err(|_| HandshakeError::StaleTimestamp)?;
    if (now_ms - ts_ms).abs() > MAX_TIMESTAMP_SKEW_MS {
        return Err(HandshakeError::StaleTimestamp);
    }

    let expected_payload = build_signed_fetch_payload("connect", "/", &timestamp, &metadata);

    let final_payload = chain
        .last()
        .map(|l| l.payload.clone())
        .ok_or(HandshakeError::NoAuthChain)?;
    if final_payload != expected_payload {
        return Err(HandshakeError::InvalidAuthChain(format!(
            "Final link rejected by policy: expected connect payload, got '{final_payload}'"
        )));
    }

    verify_auth_chain(&chain, &expected_payload, Some(now_ms))
        .map_err(|e| HandshakeError::InvalidAuthChain(e.to_string()))?;

    let user_address = signer_address(&chain)
        .ok_or_else(|| HandshakeError::InvalidAuthChain("First link must be SIGNER".into()))?;

    Ok(VerifiedHandshake {
        user_address,
        timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decentraland::pulse::HandshakeRequest;

    fn header_bag_json(links: &[(usize, &AuthLink)], ts: &str, metadata: &str) -> String {
        let mut map = serde_json::Map::new();
        for (i, link) in links {
            map.insert(
                format!("{AUTH_CHAIN_HEADER_PREFIX}{i}"),
                serde_json::Value::String(serde_json::to_string(link).unwrap()),
            );
        }
        map.insert(
            TIMESTAMP_HEADER.into(),
            serde_json::Value::String(ts.into()),
        );
        map.insert(
            METADATA_HEADER.into(),
            serde_json::Value::String(metadata.into()),
        );
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap()
    }

    #[test]
    fn signed_fetch_payload_is_lowercased() {
        assert_eq!(
            build_signed_fetch_payload("CONNECT", "/", "123", "{}"),
            "connect:/:123:{}"
        );
    }

    #[test]
    fn rejects_non_utf8_or_bad_json() {
        let req = HandshakeRequest {
            auth_chain: vec![0xFF, 0xFE],
            profile_version: 0,
            initial_state: None,
        };
        assert_eq!(
            verify_handshake(&req, 1000),
            Err(HandshakeError::InvalidJson)
        );

        let req = HandshakeRequest {
            auth_chain: b"not json".to_vec(),
            profile_version: 0,
            initial_state: None,
        };
        assert_eq!(
            verify_handshake(&req, 1000),
            Err(HandshakeError::InvalidJson)
        );
    }

    #[test]
    fn rejects_missing_auth_chain_headers() {
        let json =
            serde_json::json!({ TIMESTAMP_HEADER: "1000", METADATA_HEADER: "{}" }).to_string();
        let req = HandshakeRequest {
            auth_chain: json.into_bytes(),
            profile_version: 0,
            initial_state: None,
        };
        assert_eq!(
            verify_handshake(&req, 1000),
            Err(HandshakeError::NoAuthChain)
        );
    }

    #[test]
    fn rejects_stale_timestamp() {
        let signer = AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: "0xabc".into(),
            signature: None,
        };
        let json = header_bag_json(&[(0, &signer)], "1000", "{}");
        let req = HandshakeRequest {
            auth_chain: json.into_bytes(),
            profile_version: 0,
            initial_state: None,
        };

        assert_eq!(
            verify_handshake(&req, 1000 + MAX_TIMESTAMP_SKEW_MS + 1),
            Err(HandshakeError::StaleTimestamp)
        );
    }

    #[test]
    fn rejects_final_payload_not_connect() {
        let signer = AuthLink {
            link_type: AuthLinkType::SIGNER,
            payload: "0xabc".into(),
            signature: None,
        };
        let json = header_bag_json(&[(0, &signer)], "100000", "meta");
        let req = HandshakeRequest {
            auth_chain: json.into_bytes(),
            profile_version: 0,
            initial_state: None,
        };
        let err = verify_handshake(&req, 100000).unwrap_err();
        assert!(matches!(err, HandshakeError::InvalidAuthChain(_)));
    }

    #[tokio::test]
    async fn accepts_real_signed_chain() {
        use catalyrst_types::AuthChain as Chain;
        use ethers_signers::{LocalWallet, Signer};

        let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
        let root_addr = format!("{:#x}", root.address());
        let ephemeral: LocalWallet =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let eph_addr = format!("{:#x}", ephemeral.address());

        let ts = "1700000000000";
        let metadata = "{\"signer\":\"dcl:explorer\"}";
        let connect_payload = build_signed_fetch_payload("connect", "/", ts, metadata);

        let eph_payload = format!(
            "Decentraland Login\nEphemeral address: {eph_addr}\nExpiration: 2099-01-01T00:00:00.000Z"
        );
        let eph_sig = format!(
            "0x{}",
            root.sign_message(eph_payload.as_bytes()).await.unwrap()
        );
        let final_sig = format!(
            "0x{}",
            ephemeral
                .sign_message(connect_payload.as_bytes())
                .await
                .unwrap()
        );

        let chain: Chain = vec![
            AuthLink {
                link_type: AuthLinkType::SIGNER,
                payload: root_addr.clone(),
                signature: None,
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaEphemeral,
                payload: eph_payload,
                signature: Some(eph_sig),
            },
            AuthLink {
                link_type: AuthLinkType::EcdsaSignedEntity,
                payload: connect_payload,
                signature: Some(final_sig),
            },
        ];

        let links: Vec<(usize, &AuthLink)> = chain.iter().enumerate().collect();
        let json = header_bag_json(&links, ts, metadata);
        let req = HandshakeRequest {
            auth_chain: json.into_bytes(),
            profile_version: 0,
            initial_state: None,
        };

        let now_ms: i64 = ts.parse().unwrap();
        let ok = verify_handshake(&req, now_ms).expect("real chain must verify");
        assert_eq!(ok.user_address, root_addr.to_lowercase());
        assert_eq!(ok.timestamp, ts);

        let bad_metadata = "{\"signer\":\"dcl:other\"}";
        let bad_connect = build_signed_fetch_payload("connect", "/", ts, bad_metadata);
        let mut bad_chain = chain.clone();

        bad_chain[2].payload = bad_connect;
        let bad_links: Vec<(usize, &AuthLink)> = bad_chain.iter().enumerate().collect();
        let bad_json = header_bag_json(&bad_links, ts, metadata);
        let bad_req = HandshakeRequest {
            auth_chain: bad_json.into_bytes(),
            profile_version: 0,
            initial_state: None,
        };
        assert!(matches!(
            verify_handshake(&bad_req, now_ms).unwrap_err(),
            HandshakeError::InvalidAuthChain(_)
        ));
    }
}
