use catalyrst_crypto::signed_fetch::build_payload;
use catalyrst_crypto::Wallet;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const EPHEMERAL_DOMAIN: &[u8] = b"dcl-one upstream-bridge ephemeral v1";
const EPHEMERAL_TTL_DAYS: i64 = 30;

pub struct UpstreamIdentity {
    root: Wallet,
    ephemeral: Wallet,
    ephemeral_link: Value,
}

impl UpstreamIdentity {
    pub fn from_root_hex(raw: &str) -> anyhow::Result<Self> {
        let expiration = (chrono::Utc::now() + chrono::Duration::days(EPHEMERAL_TTL_DAYS))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        Self::from_root_hex_expiring(raw, &expiration)
    }

    pub fn from_root_hex_expiring(raw: &str, expiration: &str) -> anyhow::Result<Self> {
        let raw = raw.trim();
        let root = Wallet::from_hex(raw).map_err(|e| anyhow::anyhow!("root key: {e:?}"))?;
        let mut h = Sha256::new();
        h.update(raw.as_bytes());
        h.update(EPHEMERAL_DOMAIN);
        let ephemeral = Wallet::from_hex(&hex::encode(h.finalize()))
            .map_err(|e| anyhow::anyhow!("ephemeral key: {e:?}"))?;
        let payload = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: {}",
            ephemeral.address(),
            expiration
        );
        let signature = root
            .sign_message(payload.as_bytes())
            .map_err(|e| anyhow::anyhow!("ephemeral link: {e:?}"))?;
        let ephemeral_link = json!({
            "type": "ECDSA_EPHEMERAL",
            "payload": payload,
            "signature": signature,
        });
        Ok(Self {
            root,
            ephemeral,
            ephemeral_link,
        })
    }

    pub fn from_key_file(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_root_hex(&raw)
    }

    pub fn address(&self) -> String {
        self.root.address().to_lowercase()
    }

    pub fn ephemeral_address(&self) -> String {
        self.ephemeral.address().to_lowercase()
    }

    pub fn signed_fetch_headers(
        &self,
        method: &str,
        path: &str,
        metadata: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        self.signed_fetch_headers_at(
            method,
            path,
            metadata,
            chrono::Utc::now().timestamp_millis(),
        )
    }

    pub fn signed_fetch_headers_at(
        &self,
        method: &str,
        path: &str,
        metadata: &str,
        ts_ms: i64,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let ts = ts_ms.to_string();
        let payload = build_payload(method, path, &ts, metadata);
        let entity_sig = self
            .ephemeral
            .sign_message(payload.as_bytes())
            .map_err(|e| anyhow::anyhow!("entity sig: {e:?}"))?;
        let signer_link = json!({
            "type": "SIGNER",
            "payload": self.root.address(),
            "signature": "",
        });
        let entity_link = json!({
            "type": "ECDSA_SIGNED_ENTITY",
            "payload": payload,
            "signature": entity_sig,
        });
        Ok(vec![
            ("x-identity-auth-chain-0".into(), signer_link.to_string()),
            (
                "x-identity-auth-chain-1".into(),
                self.ephemeral_link.to_string(),
            ),
            ("x-identity-auth-chain-2".into(), entity_link.to_string()),
            ("x-identity-timestamp".into(), ts),
            ("x-identity-metadata".into(), metadata.into()),
        ])
    }

    pub fn ws_auth_frame(&self, sign_path: &str) -> anyhow::Result<String> {
        self.ws_auth_frame_at(sign_path, chrono::Utc::now().timestamp_millis())
    }

    pub fn ws_auth_frame_at(&self, sign_path: &str, ts_ms: i64) -> anyhow::Result<String> {
        let frame: Map<String, Value> = self
            .signed_fetch_headers_at("get", sign_path, "{}", ts_ms)?
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        Ok(Value::Object(frame).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::auth_chain::{verify_handshake, AuthChainError, FIVE_MINUTES_SECS};

    const KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const EXPIRATION: &str = "2099-01-01T00:00:00.000Z";
    const NOW_SECS: i64 = 1_700_000_000;

    fn identity() -> UpstreamIdentity {
        UpstreamIdentity::from_root_hex_expiring(KEY, EXPIRATION).unwrap()
    }

    #[test]
    fn derivation_is_deterministic_and_distinct_from_root() {
        let a = identity();
        let b = identity();
        assert_eq!(a.address(), b.address());
        assert_eq!(a.ephemeral_address(), b.ephemeral_address());
        assert_ne!(a.address(), a.ephemeral_address());
        assert_eq!(a.address(), "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266");
    }

    #[test]
    fn auth_frame_is_byte_stable_for_a_fixed_timestamp() {
        let a = identity().ws_auth_frame_at("/", NOW_SECS * 1000).unwrap();
        let b = identity().ws_auth_frame_at("/", NOW_SECS * 1000).unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn auth_frame_verifies_against_the_servers_own_handshake_check() {
        let id = identity();
        let frame = id.ws_auth_frame_at("/", NOW_SECS * 1000).unwrap();
        let signer = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, NOW_SECS)
            .await
            .unwrap();
        assert_eq!(signer, id.address());
    }

    #[tokio::test]
    async fn auth_frame_signed_for_one_path_fails_another() {
        let frame = identity()
            .ws_auth_frame_at("/social-rpc", NOW_SECS * 1000)
            .unwrap();
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, NOW_SECS)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
    }

    #[test]
    fn signed_fetch_headers_carry_the_full_chain_in_order() {
        let headers = identity()
            .signed_fetch_headers_at("get", "/v1/communities", "{}", NOW_SECS * 1000)
            .unwrap();
        let names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "x-identity-auth-chain-0",
                "x-identity-auth-chain-1",
                "x-identity-auth-chain-2",
                "x-identity-timestamp",
                "x-identity-metadata",
            ]
        );
        assert_eq!(headers[3].1, (NOW_SECS * 1000).to_string());
    }

    #[test]
    fn construction_rejects_a_garbage_key() {
        assert!(UpstreamIdentity::from_root_hex("nope").is_err());
    }
}
