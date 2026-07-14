//! Signed-fetch parity for the world-storage surface: which payload shape the
//! signature is checked against, and that the scene gate answers before any
//! crypto runs.

use axum::http::HeaderMap;
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_crypto::Wallet;
use catalyrst_worlds::world_storage::auth_chain::{verify_request, AuthChainError};
use serde_json::json;

const WORKER_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

const METHOD: &str = "put";
const PATH: &str = "/values/k";

fn headers_for(wallet: &Wallet, payload: &str, ts: &str, metadata: &str) -> HeaderMap {
    let signature = wallet.sign_message(payload.as_bytes()).unwrap();
    let link0 = json!({ "type": "SIGNER", "payload": wallet.address(), "signature": "" });
    let link1 =
        json!({ "type": "ECDSA_SIGNED_ENTITY", "payload": payload, "signature": signature });
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-identity-auth-chain-0",
        link0.to_string().parse().unwrap(),
    );
    headers.insert(
        "x-identity-auth-chain-1",
        link1.to_string().parse().unwrap(),
    );
    headers.insert("x-identity-timestamp", ts.parse().unwrap());
    headers.insert("x-identity-metadata", metadata.parse().unwrap());
    headers
}

fn now_ms() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

fn scene_metadata(signer: &str) -> String {
    json!({
        "signer": signer,
        "realmName": "some.dcl.eth",
        "realm": { "serverName": "some.dcl.eth" },
        "sceneId": "bafkreiabc",
        "parcel": "0,0",
    })
    .to_string()
}

async fn verify(headers: &HeaderMap) -> Result<String, AuthChainError> {
    verify_request(headers, METHOD, PATH, None)
        .await
        .map(|v| v.signer)
}

/// The crypto-middleware 6 wire format: method and path folded, metadata joined
/// verbatim. A real client on decentraland-crypto-fetch 3 signs this.
#[tokio::test]
async fn accepts_a_v6_signed_request_with_cased_metadata() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = json!({ "realmName": "Some.DCL.eth", "sceneId": "bafkreiABC" }).to_string();
    let payload = build_payload_v6(METHOD, PATH, &ts, &metadata);
    let signer = verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .expect("v6-signed request must verify");
    assert_eq!(signer.to_lowercase(), wallet.address().to_lowercase());
}

/// Our own in-tree signers (the scene-state storage proxy, the wasm explorer)
/// still mint the folded payload and cannot be shipped ahead of the server, so
/// it stays accepted behind the declared-key guard.
#[tokio::test]
async fn still_accepts_a_legacy_signed_request() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = scene_metadata("dcl:authoritative-server");
    let payload = build_legacy_payload(METHOD, PATH, &ts, &metadata);
    let signer = verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .expect("legacy-signed request must still verify");
    assert_eq!(signer.to_lowercase(), wallet.address().to_lowercase());
}

/// The folded payload leaves metadata key casing outside the signature, so a
/// re-spelled `Signer` would share the signature and read as absent to the gate.
#[tokio::test]
async fn refuses_a_legacy_request_that_respells_an_authorized_key() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata =
        json!({ "Signer": "decentraland-kernel-scene", "realmName": "some.dcl.eth" }).to_string();
    let payload = build_legacy_payload(METHOD, PATH, &ts, &metadata);
    let err = verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .expect_err("a re-spelled authorized key must not be served");
    assert!(matches!(err, AuthChainError::MalformedChain { .. }));
}

#[tokio::test]
async fn refuses_a_scene_signer() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = scene_metadata("decentraland-kernel-scene");
    let payload = build_legacy_payload(METHOD, PATH, &ts, &metadata);
    let err = verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .expect_err("a scene-signed request must never reach this surface");
    assert!(matches!(err, AuthChainError::SceneSignerRejected));
}

/// Padding is signature-bound: it was present when the payload was signed, so
/// the request is genuinely authentic and only the gate can refuse it.
#[tokio::test]
async fn refuses_a_padded_scene_signer_that_verifies() {
    for padded in [
        " decentraland-kernel-scene",
        "decentraland-kernel-scene ",
        "\tdecentraland-kernel-scene",
    ] {
        let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
        let ts = now_ms();
        let metadata = scene_metadata(padded);
        let payload = build_payload_v6(METHOD, PATH, &ts, &metadata);
        let err = verify(&headers_for(&wallet, &payload, &ts, &metadata))
            .await
            .expect_err("a padded scene signer must not read as a user-signed request");
        assert!(
            matches!(err, AuthChainError::SceneSignerRejected),
            "padded signer {padded:?} produced {err:?}"
        );
    }
}

#[tokio::test]
async fn refuses_a_recased_scene_signer_that_verifies() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = scene_metadata("Decentraland-Kernel-Scene");
    let payload = build_payload_v6(METHOD, PATH, &ts, &metadata);
    let err = verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .expect_err("a re-cased scene signer must not read as a user-signed request");
    assert!(matches!(err, AuthChainError::SceneSignerRejected));
}

/// The gate answers before signature verification, so a refused request pays no
/// catalyst round-trip for an EIP-1654 chain.
#[tokio::test]
async fn the_scene_gate_answers_before_signature_verification() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = scene_metadata("decentraland-kernel-scene");
    let err = verify(&headers_for(&wallet, "not-the-payload", &ts, &metadata))
        .await
        .expect_err("a scene-signed request must be refused");
    assert!(
        matches!(err, AuthChainError::SceneSignerRejected),
        "the gate must win over the signature check, got {err:?}"
    );
}

/// The request is genuinely signed over the timestamp it carries, so nothing but
/// the freshness check can refuse it: a timestamp the expiration window cannot be
/// computed from used to skip the window entirely and mint a credential that
/// never expired.
#[tokio::test]
async fn refuses_a_timestamp_that_is_not_plain_integer_milliseconds() {
    for ts in ["", "1.7e12", "1700000000000.0", "Infinity"] {
        let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
        let metadata = scene_metadata("dcl:authoritative-server");
        let payload = build_payload_v6(METHOD, PATH, ts, &metadata);
        let err = verify(&headers_for(&wallet, &payload, ts, &metadata))
            .await
            .expect_err("a timestamp with no computable window must not be served");
        assert!(
            matches!(&err, AuthChainError::InvalidTimestamp(value) if value == ts),
            "timestamp {ts:?} produced {err:?}"
        );
    }
}

#[tokio::test]
async fn a_canonical_non_scene_signer_is_served() {
    let wallet = Wallet::from_hex(WORKER_KEY).unwrap();
    let ts = now_ms();
    let metadata = scene_metadata("dcl:authoritative-server");
    let payload = build_payload_v6(METHOD, PATH, &ts, &metadata);
    assert!(verify(&headers_for(&wallet, &payload, &ts, &metadata))
        .await
        .is_ok());
}
