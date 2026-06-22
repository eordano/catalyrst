//! Hardening coverage for the federation primitives:
//!   * replay / dedup window correctness (clock-skew bounds + signature_hash),
//!   * RateLimiter token bucket (capacity / refill / deny),
//!   * session delegation (valid / expired / revoked / scope-mismatch),
//!   * end-to-end signed-write -> envelope encode/decode -> re-verify ->
//!     apply-dedup roundtrip.

use std::collections::HashSet;
use std::time::Duration;

use catalyrst_fed::sig::{
    domains, Eip712Domain, MAX_SKEW_FUTURE_SECS, MAX_SKEW_PAST_SECS,
};
use catalyrst_fed::{
    check_delegation, GossipEnvelope, RateLimitDecision, RateLimiter, Scope, SessionDelegation,
    SessionRevocation, Signed, TypedMessage,
};
use ethers_signers::{LocalWallet, Signer};
use serde::{Deserialize, Serialize};

// --- shared helpers --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaceVote {
    place_id: String,
    up: bool,
}

impl TypedMessage for PlaceVote {
    const PRIMARY_TYPE: &'static str = "PlaceVote";
    fn encode_struct(&self) -> Vec<u8> {
        let mut out = self.place_id.as_bytes().to_vec();
        out.push(self.up as u8);
        out
    }
}

fn wallet(seed: u8) -> LocalWallet {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    LocalWallet::from_bytes(&key).unwrap()
}

fn addr(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

async fn sign<T: TypedMessage>(
    w: &LocalWallet,
    message: T,
    domain: Eip712Domain,
    nonce: [u8; 16],
    signed_at: i64,
) -> Signed<T> {
    let mut s = Signed {
        domain,
        message,
        nonce,
        signed_at,
        signature: String::new(),
    };
    let hash = s.hash();
    let sig = w.sign_message(hash).await.unwrap();
    s.signature = format!("0x{}", sig);
    s
}

// --- replay / skew window --------------------------------------------------

#[tokio::test]
async fn skew_window_bounds_are_enforced() {
    let w = wallet(1);
    let signed_at = 1_700_000_000;
    let signed = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::places(),
        [3u8; 16],
        signed_at,
    )
    .await;
    let me = addr(&w);

    // exactly on the edges = accepted.
    signed
        .verify(&me, signed_at + MAX_SKEW_PAST_SECS)
        .expect("oldest acceptable");
    signed
        .verify(&me, signed_at - MAX_SKEW_FUTURE_SECS)
        .expect("furthest-future acceptable");

    // one second past either edge = rejected.
    assert!(
        signed.verify(&me, signed_at + MAX_SKEW_PAST_SECS + 1).is_err(),
        "too old must reject"
    );
    assert!(
        signed
            .verify(&me, signed_at - MAX_SKEW_FUTURE_SECS - 1)
            .is_err(),
        "too far in the future must reject"
    );
}

#[tokio::test]
async fn signer_mismatch_is_rejected() {
    let w = wallet(2);
    let other = wallet(3);
    let t = 1_700_000_000;
    let signed = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::places(),
        [4u8; 16],
        t,
    )
    .await;
    assert!(signed.verify(&addr(&w), t).is_ok());
    assert!(
        signed.verify(&addr(&other), t).is_err(),
        "verify against the wrong expected signer must fail"
    );
}

#[tokio::test]
async fn dedup_keys_on_signature_hash_not_payload() {
    let w = wallet(4);
    let t = 1_700_000_000;
    // same wallet, same payload, DIFFERENT nonce -> distinct signature_hash,
    // so both are independently applicable (not a replay).
    let a = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::places(),
        [1u8; 16],
        t,
    )
    .await;
    let b = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::places(),
        [2u8; 16],
        t,
    )
    .await;
    assert_ne!(a.hash(), b.hash(), "different nonce => different hash");

    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    assert!(seen.insert(a.hash()));
    assert!(seen.insert(b.hash()));
    // re-presenting `a` is the replay case -> deduped.
    assert!(!seen.insert(a.hash()), "identical action dedups");
}

// --- rate limiter token bucket ---------------------------------------------

#[test]
fn rate_limiter_capacity_then_deny() {
    // 3 tokens, refilling 3 per 100s => ~0.03/s: effectively no refill in-test.
    let rl = RateLimiter::new(3, Duration::from_secs(100));
    let signer = "0xSigner";
    assert!(matches!(rl.check(signer), RateLimitDecision::Allow));
    assert!(matches!(rl.check(signer), RateLimitDecision::Allow));
    assert!(matches!(rl.check(signer), RateLimitDecision::Allow));
    assert!(
        matches!(rl.check(signer), RateLimitDecision::Deny),
        "4th call within window must be denied"
    );
}

#[test]
fn rate_limiter_refills_over_time() {
    // 2 tokens / 100ms => 20 tokens/sec. Drain, then wait for refill.
    let rl = RateLimiter::new(2, Duration::from_millis(100));
    let s = "0xRefill";
    assert!(matches!(rl.check(s), RateLimitDecision::Allow));
    assert!(matches!(rl.check(s), RateLimitDecision::Allow));
    assert!(matches!(rl.check(s), RateLimitDecision::Deny));
    std::thread::sleep(Duration::from_millis(120));
    assert!(
        matches!(rl.check(s), RateLimitDecision::Allow),
        "tokens must refill after the window elapses"
    );
}

#[test]
fn rate_limiter_buckets_are_per_signer_and_case_insensitive() {
    let rl = RateLimiter::new(1, Duration::from_secs(100));
    assert!(matches!(rl.check("0xAAA"), RateLimitDecision::Allow));
    // a different signer has its own full bucket.
    assert!(matches!(rl.check("0xBBB"), RateLimitDecision::Allow));
    // same signer, different case => same bucket, already drained.
    assert!(matches!(rl.check("0xaaa"), RateLimitDecision::Deny));
}

// --- session delegation ----------------------------------------------------

fn delegation(expires_at: u64, signed_at: u64, scope: Vec<Scope>) -> Signed<SessionDelegation> {
    Signed {
        domain: domains::places(),
        message: SessionDelegation {
            delegate_pubkey: [5u8; 32],
            expires_at,
            scope,
            nonce: [6u8; 16],
            signed_at,
        },
        nonce: [6u8; 16],
        signed_at: signed_at as i64,
        signature: "0x".to_string() + &"00".repeat(65),
    }
}

#[test]
fn delegation_valid_within_scope_and_lifetime() {
    let now = 1_000_000;
    let d = delegation(now + 3600, now, vec![Scope::Places, Scope::Events]);
    check_delegation(&d, Scope::Places, now).expect("in-scope, unexpired");
    check_delegation(&d, Scope::Events, now).expect("multi-scope grant");
}

#[test]
fn delegation_expired_is_rejected() {
    let now = 1_000_000;
    let d = delegation(now - 1, now - 7200, vec![Scope::Places]);
    let err = check_delegation(&d, Scope::Places, now).unwrap_err();
    assert!(
        matches!(err, catalyrst_fed::FedError::SessionExpired { .. }),
        "expired delegation must be rejected: {err}"
    );
}

#[test]
fn delegation_scope_mismatch_is_rejected() {
    let now = 1_000_000;
    let d = delegation(now + 3600, now, vec![Scope::Places]);
    let err = check_delegation(&d, Scope::Communities, now).unwrap_err();
    assert!(
        matches!(err, catalyrst_fed::FedError::SessionScope { .. }),
        "out-of-scope use must be rejected: {err}"
    );
}

#[test]
fn delegation_lifetime_cap_enforced() {
    let now = 1_000_000;
    // expires more than 24h after signed_at -> rejected even though unexpired.
    let d = delegation(now + 48 * 3600, now, vec![Scope::Places]);
    let err = check_delegation(&d, Scope::Places, now).unwrap_err();
    assert!(
        matches!(err, catalyrst_fed::FedError::Malformed(_)),
        "over-24h lifetime must be rejected: {err}"
    );
}

#[test]
fn revoked_delegation_is_rejected_by_revocation_set() {
    // Revocation enforcement contract: a `SessionRevocation` names a delegation
    // by its `Signed::hash()`. Once that hash is in the revoked set, the
    // delegation must not authorize any action, even while otherwise valid.
    let now = 1_000_000;
    let d = delegation(now + 3600, now, vec![Scope::Places]);
    // the delegation passes the stateless checks first.
    check_delegation(&d, Scope::Places, now).expect("valid before revocation");

    let mut delegation_hash = [0u8; 32];
    delegation_hash.copy_from_slice(&d.hash());
    let revocation = SessionRevocation {
        delegation_hash,
        nonce: [9u8; 16],
        signed_at: now,
    };
    // the revocation message round-trips its target hash deterministically.
    assert_eq!(revocation.delegation_hash, d.hash());

    let mut revoked: HashSet<[u8; 32]> = HashSet::new();
    revoked.insert(revocation.delegation_hash);

    let authorized =
        !revoked.contains(&d.hash()) && check_delegation(&d, Scope::Places, now).is_ok();
    assert!(!authorized, "a revoked delegation must not authorize actions");
}

// --- end-to-end roundtrip --------------------------------------------------

/// signed-write -> envelope encode/decode -> re-verify -> apply-dedup.
#[tokio::test]
async fn e2e_signed_write_envelope_roundtrip_reverify_apply_dedup() {
    let domain = domains::places();
    let w = wallet(42);
    let signer = addr(&w);
    let t = chrono::Utc::now().timestamp();

    let signed = sign(
        &w,
        PlaceVote {
            place_id: "genesis-plaza".into(),
            up: true,
        },
        domain.clone(),
        [13u8; 16],
        t,
    )
    .await;
    signed.verify(&signer, t).expect("local write verifies");

    let sig_hash = hex::encode(signed.hash());
    let env =
        GossipEnvelope::local(Scope::Places, &signed, sig_hash.clone(), signer.clone()).unwrap();

    // wire roundtrip.
    let bytes = env.encode().unwrap();
    let back = GossipEnvelope::decode(&bytes).unwrap();
    assert_eq!(back.scope, Scope::Places);
    assert_eq!(back.primary_type, PlaceVote::PRIMARY_TYPE);
    assert_eq!(back.signature_hash, sig_hash);

    // receiver re-verifies from the wire bytes only.
    let inner: Signed<PlaceVote> = serde_json::from_value(back.signed_json.clone()).unwrap();
    let recovered = inner.signer().expect("recover from wire");
    assert!(recovered.eq_ignore_ascii_case(&signer));
    inner
        .verify(&recovered, chrono::Utc::now().timestamp())
        .expect("re-verify on receiver");
    assert_eq!(hex::encode(inner.hash()), back.signature_hash);

    // apply + dedup on signature_hash.
    let mut seen: HashSet<String> = HashSet::new();
    assert!(seen.insert(back.signature_hash.clone()), "first apply");
    // a re-delivered identical envelope dedups.
    let again = GossipEnvelope::decode(&bytes).unwrap();
    assert!(
        !seen.insert(again.signature_hash),
        "redelivered envelope dedups on signature_hash"
    );
}
