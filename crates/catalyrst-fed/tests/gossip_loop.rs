//! In-process federation gossip loop: publish -> consume -> re-verify -> apply
//! -> dedup, with NO live broker.
//!
//! A [`ChannelBroker`] test-double implements [`GossipPublisher`] over an
//! in-memory fan-out so two logical "nodes" can exchange envelopes exactly as
//! they would over NATS. Node A signs + applies an action locally and publishes
//! its envelope; node B re-runs the full verify/replay/authority path a local
//! write goes through and applies it once. A replay of the same envelope (same
//! `signature_hash`) is deduped to a no-op, and a forged signer is rejected.

use std::collections::HashSet;
use std::sync::Arc;

use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{GossipEnvelope, GossipPublisher, Scope, Signed, TypedMessage};
use ethers_signers::{LocalWallet, Signer};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// --- a minimal per-service action, standing in for PlaceVote/CommunityJoin ---

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

// --- in-process broker: a fan-out test-double for NATS ---------------------

/// Fan-out broker shared by every node. `publish` clones the envelope to every
/// subscriber on the same scope EXCEPT the originating peer (mirrors the
/// NatsPublisher self-echo filter), so a publisher never re-applies its own
/// writes.
#[derive(Default)]
struct Hub {
    subs: Mutex<Vec<(Scope, String, mpsc::Sender<GossipEnvelope>)>>,
}

impl Hub {
    fn register(&self, scope: Scope, peer_id: String) -> mpsc::Receiver<GossipEnvelope> {
        let (tx, rx) = mpsc::channel(64);
        self.subs.lock().push((scope, peer_id, tx));
        rx
    }

    async fn fan_out(&self, env: &GossipEnvelope) {
        let origin = env.origin_peer.clone();
        let targets: Vec<_> = self
            .subs
            .lock()
            .iter()
            .filter(|(s, peer, _)| *s == env.scope && Some(peer) != origin.as_ref())
            .map(|(_, _, tx)| tx.clone())
            .collect();
        for tx in targets {
            let _ = tx.send(env.clone()).await;
        }
    }
}

/// One node's publisher handle into the shared [`Hub`].
struct ChannelBroker {
    hub: Arc<Hub>,
    peer_id: String,
}

#[async_trait::async_trait]
impl GossipPublisher for ChannelBroker {
    async fn publish(&self, env: &GossipEnvelope) -> Result<(), catalyrst_fed::FedError> {
        let mut env = env.clone();
        if env.origin_peer.is_none() {
            env.origin_peer = Some(self.peer_id.clone());
        }
        self.hub.fan_out(&env).await;
        Ok(())
    }

    async fn subscribe(
        &self,
        scope: Scope,
    ) -> Result<Option<mpsc::Receiver<GossipEnvelope>>, catalyrst_fed::FedError> {
        Ok(Some(self.hub.register(scope, self.peer_id.clone())))
    }

    fn is_live(&self) -> bool {
        true
    }
}

// --- helpers ---------------------------------------------------------------

fn wallet(seed: u8) -> LocalWallet {
    let mut key = [0u8; 32];
    key[31] = seed;
    key[0] = 1;
    LocalWallet::from_bytes(&key).unwrap()
}

fn addr(w: &LocalWallet) -> String {
    format!("{:#x}", w.address())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
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

/// Receiving-side apply state, standing in for a service's Postgres tables:
/// `applied` = the materialised votes keyed by signature_hash; `seen` = the
/// replay dedup set (the seen_nonces table).
#[derive(Default)]
struct NodeState {
    applied: HashSet<String>,
    seen: HashSet<String>,
}

/// Re-run the full local-write path on a received envelope: decode -> verify
/// (skew + ecrecover binds the signature to the envelope's CLAIMED signer) ->
/// domain check -> hash binding -> replay dedup -> apply. Returns `Ok(true)` if
/// newly applied, `Ok(false)` if deduped, `Err` if the envelope failed
/// verification (a forged/tampered action). Mirrors places' consumer.
///
/// The trust anchor is `env.signer`: a peer claims an action was signed by that
/// wallet, and we confirm the inner signature actually recovers to it. We do NOT
/// trust whatever a tampered payload happens to recover to.
fn apply_envelope(
    state: &mut NodeState,
    domain: &Eip712Domain,
    env: &GossipEnvelope,
) -> Result<bool, String> {
    if env.primary_type != PlaceVote::PRIMARY_TYPE {
        return Err(format!("unknown primary_type {}", env.primary_type));
    }
    let signed: Signed<PlaceVote> =
        serde_json::from_value(env.signed_json.clone()).map_err(|e| e.to_string())?;
    signed.verify(&env.signer, now()).map_err(|e| e.to_string())?;
    if !signed.domain.name.eq_ignore_ascii_case(&domain.name) {
        return Err("domain mismatch".into());
    }
    // the envelope's claimed content id must match the re-derived hash, so a
    // peer cannot point a valid signature at a different dedup key.
    let key = hex::encode(signed.hash());
    if key != env.signature_hash {
        return Err("signature_hash does not bind the payload".into());
    }
    // replay dedup keyed on the canonical content id (signature_hash).
    if !state.seen.insert(key.clone()) {
        return Ok(false);
    }
    state.applied.insert(key);
    Ok(true)
}

// --- tests -----------------------------------------------------------------

#[tokio::test]
async fn publish_consume_apply_dedups_replay_across_two_nodes() {
    let hub = Arc::new(Hub::default());
    let node_a = ChannelBroker {
        hub: hub.clone(),
        peer_id: "node-a".into(),
    };
    let node_b = ChannelBroker {
        hub: hub.clone(),
        peer_id: "node-b".into(),
    };

    // node B drives the apply loop; node A is the producer.
    let mut rx_b = node_b
        .subscribe(Scope::Places)
        .await
        .unwrap()
        .expect("live broker yields a receiver");
    // node A also subscribes, to prove it does NOT receive its own echo.
    let mut rx_a = node_a.subscribe(Scope::Places).await.unwrap().unwrap();

    let domain = domains::places();
    let signer_wallet = wallet(7);
    let t = now();
    let signed = sign(
        &signer_wallet,
        PlaceVote {
            place_id: "place-1".into(),
            up: true,
        },
        domain.clone(),
        [9u8; 16],
        t,
    )
    .await;
    // sanity: the action verifies locally before A applies + publishes it.
    signed.verify(&addr(&signer_wallet), t).expect("verifies");

    let sig_hash = hex::encode(signed.hash());
    let env = GossipEnvelope::local(Scope::Places, &signed, sig_hash.clone(), addr(&signer_wallet))
        .unwrap();

    node_a.publish(&env).await.unwrap();

    // node B receives + applies exactly once.
    let mut state_b = NodeState::default();
    let got = rx_b.recv().await.expect("node B receives the envelope");
    assert_eq!(got.signature_hash, sig_hash);
    assert_eq!(got.origin_peer.as_deref(), Some("node-a"));
    assert!(
        apply_envelope(&mut state_b, &domain, &got).unwrap(),
        "first apply is fresh"
    );
    assert_eq!(state_b.applied.len(), 1);

    // a REPLAY of the very same envelope dedups to a no-op.
    node_a.publish(&env).await.unwrap();
    let replayed = rx_b.recv().await.expect("node B receives the replay");
    assert!(
        !apply_envelope(&mut state_b, &domain, &replayed).unwrap(),
        "replay with identical signature_hash must dedup"
    );
    assert_eq!(state_b.applied.len(), 1, "still applied exactly once");

    // node A never receives its own publishes (self-echo filtered by origin).
    assert!(
        rx_a.try_recv().is_err(),
        "publisher must not re-consume its own action"
    );
}

#[tokio::test]
async fn forged_signature_rejected_by_receiver() {
    let domain = domains::places();
    let real = wallet(11);
    let attacker = wallet(12);
    let t = now();

    // attacker signs, but the envelope claims the action is `real`'s.
    let mut signed = sign(
        &attacker,
        PlaceVote {
            place_id: "place-x".into(),
            up: false,
        },
        domain.clone(),
        [1u8; 16],
        t,
    )
    .await;
    // tamper: keep attacker's signature but rewrite the payload after signing.
    signed.message.place_id = "place-TAMPERED".into();

    let env = GossipEnvelope::local(Scope::Places, &signed, hex::encode(signed.hash()), addr(&real))
        .unwrap();
    let mut state = NodeState::default();
    let res = apply_envelope(&mut state, &domain, &env);
    assert!(res.is_err(), "tampered payload must fail re-verification");
    assert!(state.applied.is_empty());
}

#[tokio::test]
async fn noop_publisher_yields_no_subscription() {
    // the single-node default: no peers, no apply loop spawned.
    let p = catalyrst_fed::NoopPublisher;
    assert!(!p.is_live());
    assert!(p.subscribe(Scope::Places).await.unwrap().is_none());
}
