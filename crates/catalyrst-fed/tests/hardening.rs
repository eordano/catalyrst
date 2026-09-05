use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy::signers::{local::PrivateKeySigner, Signer};
use catalyrst_fed::consumer::{preverify_signed, spawn_gossip_consumer};
use catalyrst_fed::sig::{domains, Eip712Domain, MAX_SKEW_FUTURE_SECS, MAX_SKEW_PAST_SECS};
use catalyrst_fed::{
    check_delegation, GossipEnvelope, GossipPublisher, RateLimitDecision, RateLimiter, Scope,
    SessionDelegation, SessionRevocation, Signed, TypedMessage,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

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

fn wallet(seed: u8) -> PrivateKeySigner {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    PrivateKeySigner::from_slice(&key).unwrap()
}

fn addr(w: &PrivateKeySigner) -> String {
    format!("{:#x}", w.address())
}

async fn sign<T: TypedMessage>(
    w: &PrivateKeySigner,
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
    let sig = w.sign_message(&hash).await.unwrap();
    s.signature = sig.to_string();
    s
}

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

    signed
        .verify(&me, signed_at + MAX_SKEW_PAST_SECS)
        .expect("oldest acceptable");
    signed
        .verify(&me, signed_at - MAX_SKEW_FUTURE_SECS)
        .expect("furthest-future acceptable");

    assert!(
        signed
            .verify(&me, signed_at + MAX_SKEW_PAST_SECS + 1)
            .is_err(),
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

    assert!(!seen.insert(a.hash()), "identical action dedups");
}

#[test]
fn rate_limiter_capacity_then_deny() {
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

    assert!(matches!(rl.check("0xBBB"), RateLimitDecision::Allow));

    assert!(matches!(rl.check("0xaaa"), RateLimitDecision::Deny));
}

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

    let d = delegation(now + 48 * 3600, now, vec![Scope::Places]);
    let err = check_delegation(&d, Scope::Places, now).unwrap_err();
    assert!(
        matches!(err, catalyrst_fed::FedError::Malformed(_)),
        "over-24h lifetime must be rejected: {err}"
    );
}

#[test]
fn revoked_delegation_is_rejected_by_revocation_set() {
    let now = 1_000_000;
    let d = delegation(now + 3600, now, vec![Scope::Places]);

    check_delegation(&d, Scope::Places, now).expect("valid before revocation");

    let mut delegation_hash = [0u8; 32];
    delegation_hash.copy_from_slice(&d.hash());
    let revocation = SessionRevocation {
        delegation_hash,
        nonce: [9u8; 16],
        signed_at: now,
    };

    assert_eq!(revocation.delegation_hash, d.hash());

    let mut revoked: HashSet<[u8; 32]> = HashSet::new();
    revoked.insert(revocation.delegation_hash);

    let authorized =
        !revoked.contains(&d.hash()) && check_delegation(&d, Scope::Places, now).is_ok();
    assert!(
        !authorized,
        "a revoked delegation must not authorize actions"
    );
}

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

    let bytes = env.encode().unwrap();
    let back = GossipEnvelope::decode(&bytes).unwrap();
    assert_eq!(back.scope, Scope::Places);
    assert_eq!(back.primary_type, PlaceVote::PRIMARY_TYPE);
    assert_eq!(back.signature_hash, sig_hash);

    let inner: Signed<PlaceVote> = serde_json::from_value(back.signed_json.clone()).unwrap();
    let recovered = inner.signer().expect("recover from wire");
    assert!(recovered.eq_ignore_ascii_case(&signer));
    inner
        .verify(&recovered, chrono::Utc::now().timestamp())
        .expect("re-verify on receiver");
    assert_eq!(hex::encode(inner.hash()), back.signature_hash);

    let mut seen: HashSet<String> = HashSet::new();
    assert!(seen.insert(back.signature_hash.clone()), "first apply");

    let again = GossipEnvelope::decode(&bytes).unwrap();
    assert!(
        !seen.insert(again.signature_hash),
        "redelivered envelope dedups on signature_hash"
    );
}

#[tokio::test]
async fn preverify_signed_accepts_valid_envelope_and_runs_replay_hook() {
    let w = wallet(21);
    let t = chrono::Utc::now().timestamp();
    let signed = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::places(),
        [8u8; 16],
        t,
    )
    .await;

    let recorded: Mutex<Option<(String, [u8; 16], i64)>> = Mutex::new(None);
    let signer = preverify_signed(&signed, "DecentralandPlaces", |signer, nonce, signed_at| {
        *recorded.lock().unwrap() = Some((signer, nonce, signed_at));
        async { Ok::<(), catalyrst_fed::FedError>(()) }
    })
    .await
    .expect("valid envelope must preverify");

    assert!(signer.eq_ignore_ascii_case(&addr(&w)));
    let (hook_signer, hook_nonce, hook_signed_at) = recorded
        .lock()
        .unwrap()
        .take()
        .expect("replay hook must run on the happy path");
    assert!(hook_signer.eq_ignore_ascii_case(&addr(&w)));
    assert_eq!(hook_nonce, [8u8; 16]);
    assert_eq!(hook_signed_at, t);
}

#[tokio::test]
async fn preverify_signed_rejects_events_domain_on_places_verifier() {
    let w = wallet(22);
    let t = chrono::Utc::now().timestamp();
    let signed = sign(
        &w,
        PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        domains::events(),
        [9u8; 16],
        t,
    )
    .await;

    let replay_ran = AtomicBool::new(false);
    let err = preverify_signed(&signed, "DecentralandPlaces", |_, _, _| {
        replay_ran.store(true, Ordering::SeqCst);
        async { Ok::<(), catalyrst_fed::FedError>(()) }
    })
    .await
    .expect_err("cross-domain envelope must be rejected");

    assert!(err.contains("domain mismatch"), "{err}");
    assert!(
        !replay_ran.load(Ordering::SeqCst),
        "replay hook must not run for a rejected domain"
    );
}

struct InjectedFeed(Mutex<Option<mpsc::Receiver<GossipEnvelope>>>);

#[async_trait::async_trait]
impl GossipPublisher for InjectedFeed {
    async fn publish(&self, _env: &GossipEnvelope) -> Result<(), catalyrst_fed::FedError> {
        Ok(())
    }

    async fn subscribe(
        &self,
        _scope: Scope,
    ) -> Result<Option<mpsc::Receiver<GossipEnvelope>>, catalyrst_fed::FedError> {
        Ok(self.0.lock().unwrap().take())
    }
}

#[tokio::test]
async fn consumer_rejects_wrong_scope_envelope_before_dispatch() {
    let (feed_tx, feed_rx) = mpsc::channel(8);
    let publisher = Arc::new(InjectedFeed(Mutex::new(Some(feed_rx))));
    let (dispatched_tx, mut dispatched_rx) = mpsc::channel::<String>(8);

    spawn_gossip_consumer(publisher, Scope::Places, move |env: GossipEnvelope| {
        let tx = dispatched_tx.clone();
        async move { tx.send(env.signature_hash).await.map_err(|e| e.to_string()) }
    })
    .await;

    let signed = Signed {
        domain: domains::places(),
        message: PlaceVote {
            place_id: "p".into(),
            up: true,
        },
        nonce: [7u8; 16],
        signed_at: 1_700_000_000,
        signature: "0x".to_string() + &"00".repeat(65),
    };
    let stray =
        GossipEnvelope::local(Scope::Events, &signed, "stray".into(), "0x1".into()).unwrap();
    let good = GossipEnvelope::local(Scope::Places, &signed, "good".into(), "0x1".into()).unwrap();
    feed_tx.send(stray).await.unwrap();
    feed_tx.send(good).await.unwrap();

    let first = dispatched_rx
        .recv()
        .await
        .expect("in-scope envelope must dispatch");
    assert_eq!(
        first, "good",
        "wrong-scope envelope must never reach dispatch"
    );
    assert!(dispatched_rx.try_recv().is_err());
}
