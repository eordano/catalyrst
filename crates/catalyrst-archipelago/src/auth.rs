use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::AuthChain;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use crate::config::AuthConfig;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("challenge not found or expired")]
    UnknownChallenge,
    #[error("challenge does not match")]
    ChallengeMismatch,
    #[error("auth chain rejected: {0}")]
    InvalidChain(String),
    #[error("signature too old: {0}s")]
    SignatureTooOld(i64),
    #[error("address mismatch")]
    AddressMismatch,
}

#[derive(Clone, Debug)]
struct Issued {
    challenge: String,
    issued_at: DateTime<Utc>,
}

pub struct ChallengeStore {
    cfg: AuthConfig,
    by_address: Mutex<HashMap<String, Issued>>,
}

impl ChallengeStore {
    pub fn new(cfg: AuthConfig) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            by_address: Mutex::new(HashMap::new()),
        })
    }

    pub fn required(&self) -> bool {
        self.cfg.require_signed_challenge
    }

    pub fn put(&self, address: &str, challenge: &str) {
        let mut guard = self.by_address.lock();
        let addr_lc = address.to_ascii_lowercase();
        self.gc(&mut guard);
        guard.insert(
            addr_lc,
            Issued {
                challenge: challenge.to_string(),
                issued_at: Utc::now(),
            },
        );
    }

    pub fn issue(&self, address: &str) -> String {
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        let challenge = hex_encode(&bytes);
        let mut guard = self.by_address.lock();
        let addr_lc = address.to_ascii_lowercase();
        self.gc(&mut guard);
        guard.insert(
            addr_lc,
            Issued {
                challenge: challenge.clone(),
                issued_at: Utc::now(),
            },
        );
        challenge
    }

    pub fn redeem_and_verify(
        &self,
        address: &str,
        challenge: &str,
        chain: &AuthChain,
    ) -> Result<(), AuthError> {
        let addr_lc = address.to_ascii_lowercase();
        let issued = {
            let mut guard = self.by_address.lock();
            self.gc(&mut guard);
            guard.remove(&addr_lc).ok_or(AuthError::UnknownChallenge)?
        };
        if issued.challenge != challenge {
            return Err(AuthError::ChallengeMismatch);
        }
        let now = Utc::now();
        let age = now.signed_duration_since(issued.issued_at).num_seconds();
        if age > self.cfg.signature_max_age_secs as i64 {
            return Err(AuthError::SignatureTooOld(age));
        }
        if chain.is_empty() {
            return Err(AuthError::InvalidChain("empty chain".into()));
        }
        if let Some(first) = chain.first() {
            if !first.payload.eq_ignore_ascii_case(address) {
                return Err(AuthError::AddressMismatch);
            }
        }
        let last_payload = chain.last().map(|l| l.payload.clone()).unwrap_or_default();
        if last_payload != challenge {
            return Err(AuthError::ChallengeMismatch);
        }
        // The chain's final authority is the signed payload (the challenge text),
        // not the wallet address — verify_auth_chain walks SIGNER -> EPHEMERAL ->
        // SIGNED_ENTITY(payload=challenge). The address binding is enforced by the
        // first-link check above.
        verify_auth_chain(chain, challenge, Some(now.timestamp_millis()))
            .map_err(|e| AuthError::InvalidChain(format!("{:?}", e)))?;
        Ok(())
    }

    fn gc(&self, guard: &mut HashMap<String, Issued>) {
        let cutoff = Utc::now() - chrono::Duration::seconds(self.cfg.challenge_ttl_secs as i64);
        guard.retain(|_, v| v.issued_at >= cutoff);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(*b >> 4) as usize] as char);
        out.push(HEX[(*b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(require: bool) -> AuthConfig {
        AuthConfig {
            require_signed_challenge: require,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
        }
    }

    #[test]
    fn challenge_is_random_per_call() {
        let s = ChallengeStore::new(cfg(true));
        let a = s.issue("0x0000000000000000000000000000000000000001");
        let b = s.issue("0x0000000000000000000000000000000000000002");
        assert_ne!(a, b);
        assert_eq!(a.len(), 48);
    }

    #[test]
    fn redeem_with_unknown_address_fails() {
        let s = ChallengeStore::new(cfg(true));
        let err = s
            .redeem_and_verify(
                "0x0000000000000000000000000000000000000001",
                "deadbeef",
                &vec![],
            )
            .unwrap_err();
        matches!(err, AuthError::UnknownChallenge);
    }

    /// Happy path with a real signed chain (final authority = challenge text).
    #[test]
    fn redeem_with_valid_signed_chain_succeeds() {
        use ethers_signers::{LocalWallet, Signer};

        let s = ChallengeStore::new(cfg(true));
        let wallet = LocalWallet::new(&mut ethers_core::rand::thread_rng());
        let address = format!("{:#x}", wallet.address());

        let challenge = s.issue(&address);

        let hash = ethers_core::utils::hash_message(challenge.as_bytes());
        let sig = wallet.sign_hash(hash).expect("sign");

        let chain: AuthChain = serde_json::from_value(serde_json::json!([
            { "type": "SIGNER", "payload": address, "signature": "" },
            {
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": challenge,
                "signature": format!("0x{}", sig)
            }
        ]))
        .expect("chain json");

        s.redeem_and_verify(&address, &challenge, &chain)
            .expect("valid signed challenge must verify");
    }

    /// A chain signed by a DIFFERENT wallet than the claimed address must fail.
    #[test]
    fn redeem_with_wrong_signer_fails() {
        use ethers_signers::{LocalWallet, Signer};

        let s = ChallengeStore::new(cfg(true));
        let wallet = LocalWallet::new(&mut ethers_core::rand::thread_rng());
        let impostor = LocalWallet::new(&mut ethers_core::rand::thread_rng());
        let address = format!("{:#x}", wallet.address());

        let challenge = s.issue(&address);
        let hash = ethers_core::utils::hash_message(challenge.as_bytes());
        let sig = impostor.sign_hash(hash).expect("sign");

        let chain: AuthChain = serde_json::from_value(serde_json::json!([
            { "type": "SIGNER", "payload": address, "signature": "" },
            {
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": challenge,
                "signature": format!("0x{}", sig)
            }
        ]))
        .expect("chain json");

        s.redeem_and_verify(&address, &challenge, &chain)
            .expect_err("impostor signature must be rejected");
    }
}
