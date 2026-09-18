use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_types::AuthChain;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rand::Rng;
use std::collections::{HashMap, VecDeque};
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

impl AuthError {
    pub fn class(&self) -> &'static str {
        match self {
            Self::UnknownChallenge => "unknown_challenge",
            Self::ChallengeMismatch => "challenge_mismatch",
            Self::InvalidChain(_) => "invalid_chain",
            Self::SignatureTooOld(_) => "signature_too_old",
            Self::AddressMismatch => "address_mismatch",
        }
    }
}

pub const MAX_PENDING_CHALLENGES: usize = 65_536;

type ChallengeKey = (String, String);

#[derive(Clone, Debug)]
struct Issued {
    issued_at: DateTime<Utc>,
    sequence: u64,
}

/// Anyone can ask for a challenge, so the pending set is bounded: past
/// `MAX_PENDING_CHALLENGES` the oldest live challenge leaves first. `order` holds issuance
/// order; an entry whose sequence no longer matches the map is a redeemed or discarded
/// challenge and holds no capacity.
#[derive(Default)]
struct Pending {
    by_challenge: HashMap<ChallengeKey, Issued>,
    order: VecDeque<(ChallengeKey, u64)>,
    next_sequence: u64,
}

impl Pending {
    fn pop_oldest(&mut self, expired_before: Option<DateTime<Utc>>) -> bool {
        let Some((key, sequence)) = self.order.front() else {
            return false;
        };
        let live = self
            .by_challenge
            .get(key)
            .filter(|issued| issued.sequence == *sequence);
        if let (Some(issued), Some(cutoff)) = (live, expired_before) {
            if issued.issued_at >= cutoff {
                return false;
            }
        }
        if live.is_some() {
            self.by_challenge.remove(key);
        }
        self.order.pop_front();
        true
    }
}

pub struct ChallengeStore {
    cfg: AuthConfig,
    pending: Mutex<Pending>,
    socket_pending: Mutex<Pending>,
}

impl ChallengeStore {
    pub fn new(cfg: AuthConfig) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            pending: Mutex::new(Pending::default()),
            socket_pending: Mutex::new(Pending::default()),
        })
    }

    pub fn required(&self) -> bool {
        self.cfg.require_signed_challenge
    }

    pub fn put(&self, address: &str, challenge: &str) {
        self.put_in(&self.pending, address, challenge);
    }

    fn put_in(&self, set: &Mutex<Pending>, address: &str, challenge: &str) {
        let mut guard = set.lock();
        let pending = &mut *guard;
        let cutoff = self.cutoff();
        while pending.pop_oldest(Some(cutoff)) {}
        while pending.by_challenge.len() >= MAX_PENDING_CHALLENGES && pending.pop_oldest(None) {}
        if pending.order.len() > 2 * MAX_PENDING_CHALLENGES {
            let live = &pending.by_challenge;
            pending.order.retain(|(key, sequence)| {
                live.get(key)
                    .is_some_and(|issued| issued.sequence == *sequence)
            });
        }
        let key = (address.to_ascii_lowercase(), challenge.to_string());
        let sequence = pending.next_sequence;
        pending.next_sequence += 1;
        pending.order.push_back((key.clone(), sequence));
        pending.by_challenge.insert(
            key,
            Issued {
                issued_at: Utc::now(),
                sequence,
            },
        );
    }

    pub fn issue(&self, address: &str) -> String {
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        let challenge = hex_encode(&bytes);
        self.put(address, &challenge);
        challenge
    }

    pub fn discard(&self, address: &str, challenge: &str) {
        Self::discard_in(&self.pending, address, challenge);
    }

    fn discard_in(set: &Mutex<Pending>, address: &str, challenge: &str) {
        set.lock()
            .by_challenge
            .remove(&(address.to_ascii_lowercase(), challenge.to_string()));
    }

    pub fn redeem_and_verify(
        &self,
        address: &str,
        challenge: &str,
        chain: &AuthChain,
    ) -> Result<(), AuthError> {
        self.redeem_in(&self.pending, address, address, challenge, challenge, chain)
    }

    pub fn put_for_key(&self, key: &str, challenge: &str) {
        self.put_in(&self.socket_pending, key, challenge);
    }

    pub fn discard_for_key(&self, key: &str, challenge: &str) {
        Self::discard_in(&self.socket_pending, key, challenge);
    }

    pub fn redeem_and_verify_payload(
        &self,
        storage_key: &str,
        address: &str,
        challenge: &str,
        expected_payload: &str,
        chain: &AuthChain,
    ) -> Result<(), AuthError> {
        self.redeem_in(
            &self.socket_pending,
            storage_key,
            address,
            challenge,
            expected_payload,
            chain,
        )
    }

    fn redeem_in(
        &self,
        set: &Mutex<Pending>,
        storage_key: &str,
        address: &str,
        challenge: &str,
        expected_payload: &str,
        chain: &AuthChain,
    ) -> Result<(), AuthError> {
        let addr_lc = address.to_ascii_lowercase();
        let key = (storage_key.to_ascii_lowercase(), challenge.to_string());
        let issued = set
            .lock()
            .by_challenge
            .get(&key)
            .filter(|issued| issued.issued_at >= self.cutoff())
            .cloned()
            .ok_or(AuthError::UnknownChallenge)?;
        let now = Utc::now();
        let age = now.signed_duration_since(issued.issued_at).num_seconds();
        if age > self.cfg.signature_max_age_secs as i64 {
            return Err(AuthError::SignatureTooOld(age));
        }
        if chain.is_empty() {
            return Err(AuthError::InvalidChain("empty chain".into()));
        }
        if let Some(first) = chain.first() {
            if !first.payload.eq_ignore_ascii_case(&addr_lc) {
                return Err(AuthError::AddressMismatch);
            }
        }
        let last_payload = chain.last().map(|l| l.payload.clone()).unwrap_or_default();
        if last_payload != expected_payload {
            return Err(AuthError::ChallengeMismatch);
        }

        verify_auth_chain(chain, expected_payload, Some(now.timestamp_millis()))
            .map_err(|e| AuthError::InvalidChain(format!("{:?}", e)))?;
        let mut guard = set.lock();
        match guard.by_challenge.get(&key) {
            Some(current)
                if current.sequence == issued.sequence && current.issued_at >= self.cutoff() =>
            {
                guard.by_challenge.remove(&key);
                Ok(())
            }
            _ => Err(AuthError::UnknownChallenge),
        }
    }

    fn cutoff(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::seconds(self.cfg.challenge_ttl_secs as i64)
    }

    #[cfg(test)]
    fn pending(&self) -> usize {
        self.pending.lock().by_challenge.len()
    }

    #[cfg(test)]
    fn queued(&self) -> usize {
        self.pending.lock().order.len()
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

    #[test]
    fn rejection_class_never_retains_invalid_chain_details() {
        const CANARY: &str = "archipelago-invalid-chain-private-material-canary";
        let error = AuthError::InvalidChain(CANARY.to_string());
        assert_eq!(error.class(), "invalid_chain");
        assert!(!error.class().contains(CANARY));
    }

    fn cfg(require: bool) -> AuthConfig {
        AuthConfig {
            require_signed_challenge: require,
            challenge_ttl_secs: 120,
            signature_max_age_secs: 300,
            deny_list_url: None,
            ..AuthConfig::default()
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
        assert!(
            matches!(err, AuthError::UnknownChallenge),
            "expected UnknownChallenge, got {err:?}"
        );
    }

    #[test]
    fn redeem_with_valid_signed_chain_succeeds() {
        use alloy::signers::{local::PrivateKeySigner, SignerSync};

        let s = ChallengeStore::new(cfg(true));
        let wallet = PrivateKeySigner::random();
        let address = format!("{:#x}", wallet.address());

        let challenge = s.issue(&address);

        let hash = alloy::primitives::eip191_hash_message(challenge.as_bytes());
        let sig = wallet.sign_hash_sync(&hash).expect("sign");

        let chain: AuthChain = serde_json::from_value(serde_json::json!([
            { "type": "SIGNER", "payload": address, "signature": "" },
            {
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": challenge,
                "signature": sig.to_string()
            }
        ]))
        .expect("chain json");

        s.redeem_and_verify(&address, &challenge, &chain)
            .expect("valid signed challenge must verify");
    }

    #[test]
    fn redeem_with_wrong_signer_fails() {
        use alloy::signers::{local::PrivateKeySigner, SignerSync};

        let s = ChallengeStore::new(cfg(true));
        let wallet = PrivateKeySigner::random();
        let impostor = PrivateKeySigner::random();
        let address = format!("{:#x}", wallet.address());

        let challenge = s.issue(&address);
        let hash = alloy::primitives::eip191_hash_message(challenge.as_bytes());
        let sig = impostor.sign_hash_sync(&hash).expect("sign");

        let chain: AuthChain = serde_json::from_value(serde_json::json!([
            { "type": "SIGNER", "payload": address, "signature": "" },
            {
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": challenge,
                "signature": sig.to_string()
            }
        ]))
        .expect("chain json");

        s.redeem_and_verify(&address, &challenge, &chain)
            .expect_err("impostor signature must be rejected");
    }

    fn signed(signer: &alloy::signers::local::PrivateKeySigner, challenge: &str) -> AuthChain {
        use alloy::signers::SignerSync;
        serde_json::from_value(serde_json::json!([
            { "type": "SIGNER", "payload": format!("{:#x}", signer.address()), "signature": "" },
            { "type": "ECDSA_SIGNED_ENTITY", "payload": challenge,
              "signature": signer.sign_message_sync(challenge.as_bytes()).unwrap().to_string() }
        ]))
        .unwrap()
    }

    #[test]
    fn same_wallet_challenges_survive_mismatch_and_invalid_signature() {
        use alloy::signers::local::PrivateKeySigner;
        let store = ChallengeStore::new(cfg(true));
        let signer = PrivateKeySigner::random();
        let address = format!("{:#x}", signer.address());
        let first = store.issue(&address);
        let second = store.issue(&address.to_uppercase());
        let chain = signed(&signer, &first);
        assert!(matches!(
            store.redeem_and_verify(&address, "absent", &chain),
            Err(AuthError::UnknownChallenge)
        ));
        assert!(matches!(
            store.redeem_and_verify(&address, &second, &chain),
            Err(AuthError::ChallengeMismatch)
        ));
        assert!(store
            .redeem_and_verify(
                &address,
                &first,
                &signed(&PrivateKeySigner::random(), &first)
            )
            .is_err());
        store.redeem_and_verify(&address, &first, &chain).unwrap();
        store
            .redeem_and_verify(&address, &second, &signed(&signer, &second))
            .unwrap();
        assert!(matches!(
            store.redeem_and_verify(&address, &first, &chain),
            Err(AuthError::UnknownChallenge)
        ));
    }

    #[test]
    fn concurrent_redemptions_commit_exactly_once() {
        use alloy::signers::local::PrivateKeySigner;
        let store = ChallengeStore::new(cfg(true));
        let signer = PrivateKeySigner::random();
        let address = format!("{:#x}", signer.address());
        let challenge = store.issue(&address);
        let chain = signed(&signer, &challenge);
        let barrier = std::sync::Barrier::new(8);
        let successes = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        store
                            .redeem_and_verify(&address, &challenge, &chain)
                            .is_ok()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| usize::from(h.join().unwrap()))
                .sum::<usize>()
        });
        assert_eq!(successes, 1);
    }

    fn numbered(n: usize) -> String {
        format!("0x{n:040x}")
    }

    #[test]
    fn unredeemed_challenges_are_bounded_and_the_oldest_leave_first() {
        let store = ChallengeStore::new(cfg(true));
        let oldest = store.issue(&numbered(0));
        let mut newest = String::new();
        for n in 1..=MAX_PENDING_CHALLENGES {
            newest = store.issue(&numbered(n));
        }
        assert_eq!(store.pending(), MAX_PENDING_CHALLENGES);
        assert!(matches!(
            store.redeem_and_verify(&numbered(0), &oldest, &vec![]),
            Err(AuthError::UnknownChallenge)
        ));
        assert!(
            matches!(
                store.redeem_and_verify(&numbered(MAX_PENDING_CHALLENGES), &newest, &vec![]),
                Err(AuthError::InvalidChain(_))
            ),
            "the newest challenge must still be known"
        );
    }

    #[test]
    fn a_flood_of_requested_challenges_never_evicts_one_a_socket_is_waiting_on() {
        use alloy::signers::local::PrivateKeySigner;
        let store = ChallengeStore::new(cfg(true));
        let signer = PrivateKeySigner::random();
        let address = format!("{:#x}", signer.address());
        store.put_for_key("socket-1", "socket-challenge");
        for n in 0..=MAX_PENDING_CHALLENGES {
            store.issue(&numbered(n));
        }
        assert_eq!(store.pending(), MAX_PENDING_CHALLENGES);
        assert!(store
            .redeem_and_verify_payload(
                "socket-1",
                &address,
                "socket-challenge",
                "transcript",
                &signed(&signer, "transcript"),
            )
            .is_ok());

        let requested = store.issue(&address);
        for n in 0..=MAX_PENDING_CHALLENGES {
            store.put_for_key(&format!("socket-{n}"), "churn");
        }
        assert!(store
            .redeem_and_verify(&address, &requested, &signed(&signer, &requested))
            .is_ok());
        assert!(matches!(
            store.redeem_and_verify_payload(&address, &address, &requested, &requested, &vec![]),
            Err(AuthError::UnknownChallenge)
        ));
    }

    #[test]
    fn discarded_challenges_neither_hold_capacity_nor_evict_a_live_one() {
        let store = ChallengeStore::new(cfg(true));
        let live = store.issue(&numbered(0));
        for n in 1..=3 * MAX_PENDING_CHALLENGES {
            let churned = store.issue(&numbered(n));
            store.discard(&numbered(n), &churned);
        }
        assert_eq!(store.pending(), 1);
        assert!(store.queued() <= 2 * MAX_PENDING_CHALLENGES + 1);
        assert!(matches!(
            store.redeem_and_verify(&numbered(0), &live, &vec![]),
            Err(AuthError::InvalidChain(_))
        ));
    }

    #[test]
    fn an_expired_challenge_is_unknown_even_before_it_is_collected() {
        let store = ChallengeStore::new(AuthConfig {
            challenge_ttl_secs: 0,
            ..cfg(true)
        });
        let challenge = store.issue(&numbered(1));
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(matches!(
            store.redeem_and_verify(&numbered(1), &challenge, &vec![]),
            Err(AuthError::UnknownChallenge)
        ));
    }

    #[test]
    fn discarding_one_socket_challenge_preserves_another() {
        use alloy::signers::local::PrivateKeySigner;
        let store = ChallengeStore::new(cfg(true));
        let signer = PrivateKeySigner::random();
        let address = format!("{:#x}", signer.address());
        let first = store.issue(&address);
        let second = store.issue(&address);
        store.discard(&address, &first);
        assert!(matches!(
            store.redeem_and_verify(&address, &first, &signed(&signer, &first)),
            Err(AuthError::UnknownChallenge)
        ));
        store
            .redeem_and_verify(&address, &second, &signed(&signer, &second))
            .unwrap();
    }
}
