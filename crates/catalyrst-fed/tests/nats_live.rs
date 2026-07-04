#![cfg(feature = "nats")]

use catalyrst_fed::gossip::nats::NatsPublisher;
use catalyrst_fed::sig::{domains, Eip712Domain};
use catalyrst_fed::{GossipEnvelope, GossipPublisher, Scope, Signed, TypedMessage};
use ethers_signers::{LocalWallet, Signer};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

async fn sign(
    w: &LocalWallet,
    message: PlaceVote,
    domain: Eip712Domain,
    nonce: [u8; 16],
    signed_at: i64,
) -> Signed<PlaceVote> {
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

#[tokio::test]
async fn nats_publish_then_consume_roundtrip() {
    let Ok(url) = std::env::var("FED_NATS_URL") else {
        eprintln!("skipping nats_publish_then_consume_roundtrip: FED_NATS_URL not set");
        return;
    };

    let node_a = match NatsPublisher::connect(&url, "test-node-a".into()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping nats_publish_then_consume_roundtrip: connect failed: {e}");
            return;
        }
    };
    let node_b = NatsPublisher::connect(&url, "test-node-b".into())
        .await
        .expect("node B connect");

    let mut rx = node_b
        .subscribe(Scope::Places)
        .await
        .expect("subscribe")
        .expect("live transport yields a receiver");

    let mut key = [0u8; 32];
    key[31] = 7;
    key[0] = 1;
    let w = LocalWallet::from_bytes(&key).unwrap();
    let signer = format!("{:#x}", w.address());
    let t = chrono::Utc::now().timestamp();

    let mut nonce = [0u8; 16];
    nonce[..8].copy_from_slice(&t.to_be_bytes());
    let signed = sign(
        &w,
        PlaceVote {
            place_id: format!("place-{t}"),
            up: true,
        },
        domains::places(),
        nonce,
        t,
    )
    .await;
    let sig_hash = hex::encode(signed.hash());
    let env = GossipEnvelope::local(Scope::Places, &signed, sig_hash.clone(), signer).unwrap();

    node_a.publish(&env).await.expect("publish");

    let got = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Some(e) if e.signature_hash == sig_hash => break Some(e),
                Some(_) => continue,
                None => break None,
            }
        }
    })
    .await
    .expect("timed out waiting for gossip envelope")
    .expect("channel closed before envelope arrived");

    assert_eq!(got.origin_peer.as_deref(), Some("test-node-a"));
    let inner: Signed<PlaceVote> = serde_json::from_value(got.signed_json).unwrap();
    let recovered = inner.signer().expect("recover");
    inner
        .verify(&recovered, chrono::Utc::now().timestamp())
        .expect("re-verify off the live wire");
}
