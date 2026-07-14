//! Signed-fetch parity for the worlds surface: the 6.x payload is what the
//! signature is checked against, the folded payload stays accepted only behind
//! each route's declared keys, and the scene gate answers before any crypto.

use axum::http::HeaderMap;
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_crypto::Wallet;
use catalyrst_worlds::auth_chain::{
    require_verified, AuthChainError, VerifiedAuth, EXPLORER_METADATA_KEYS, KERNEL_SCENE_SIGNER,
    PERMISSIONS_METADATA_KEYS,
};
use serde_json::json;

const EXPLORER_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

const METHOD: &str = "post";
const COMMS_PATH: &str = "/worlds/some.dcl.eth/comms";
const PERMISSIONS_PATH: &str = "/world/some.dcl.eth/permissions/access";
const SETTINGS_PATH: &str = "/world/some.dcl.eth/settings";

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

/// The handshake metadata the bevy, unity and godot explorers send: camelCase,
/// so the folded payload and the delivered bytes differ.
fn explorer_metadata(signer: &str) -> String {
    json!({
        "origin": "https://play.decentraland.org",
        "intent": "dcl:explorer:comms-handshake",
        "signer": signer,
        "isGuest": "false",
    })
    .to_string()
}

async fn verify(
    headers: &HeaderMap,
    path: &str,
    keys: &[&str],
) -> Result<VerifiedAuth, AuthChainError> {
    require_verified(headers, METHOD, path, keys).await
}

fn signed_by(auth: &VerifiedAuth, wallet: &Wallet) -> bool {
    auth.signer.as_str().eq_ignore_ascii_case(&wallet.address())
}

/// The crypto-middleware 6 wire format: method and path folded, metadata joined
/// verbatim. A client on decentraland-crypto-fetch 3 signs this.
#[tokio::test]
async fn accepts_a_v6_signed_handshake_with_cased_metadata() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata = explorer_metadata("dcl:explorer");
    let payload = build_payload_v6(METHOD, COMMS_PATH, &ts, &metadata);
    let auth = verify(
        &headers_for(&wallet, &payload, &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .expect("v6-signed handshake must verify");
    assert!(signed_by(&auth, &wallet));
}

/// The explorers cannot be shipped ahead of this server, so the folded payload
/// they still sign stays accepted on the handshake routes.
#[tokio::test]
async fn still_accepts_the_folded_payload_every_explorer_signs() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata = explorer_metadata("dcl:explorer");
    let payload = build_legacy_payload(METHOD, COMMS_PATH, &ts, &metadata);
    let auth = verify(
        &headers_for(&wallet, &payload, &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .expect("legacy-signed handshake must still verify");
    assert!(signed_by(&auth, &wallet));
    assert_eq!(auth.metadata["isGuest"], "false");
}

/// The folded payload leaves key casing outside the signature, so every key a
/// handshake route authorizes on is pinned to its declared spelling before the
/// fallback is consulted; `signer` is caught one layer earlier by the gate.
#[tokio::test]
async fn refuses_a_legacy_handshake_that_respells_a_declared_key() {
    for (declared, respelled, guard) in [
        ("signer", "Signer", "invalid metadata content"),
        ("intent", "Intent", "expected \"intent\""),
        ("secret", "Secret", "expected \"secret\""),
    ] {
        let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
        let ts = now_ms();
        let mut object: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&explorer_metadata("dcl:explorer")).unwrap();
        let value = object.remove(declared).unwrap_or_else(|| json!("hunter2"));
        object.insert(respelled.to_string(), value);
        let metadata = serde_json::Value::Object(object).to_string();
        let payload = build_legacy_payload(METHOD, COMMS_PATH, &ts, &metadata);
        let err = verify(
            &headers_for(&wallet, &payload, &ts, &metadata),
            COMMS_PATH,
            EXPLORER_METADATA_KEYS,
        )
        .await
        .expect_err("a re-spelled declared key must not be served");
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{respelled:?} produced {err:?}"
        );
        assert!(
            err.http_message().contains(guard),
            "{respelled:?} must be refused by the {guard:?} guard, got {}",
            err.http_message()
        );
    }
}

/// Keys the route never reads are not pinned: a legacy handshake spelling one
/// of them differently is served, exactly as upstream serves it.
#[tokio::test]
async fn leaves_undeclared_legacy_keys_alone() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata = json!({
        "Origin": "https://play.decentraland.org",
        "intent": "dcl:explorer:comms-handshake",
        "signer": "dcl:explorer",
        "IsGuest": "false",
    })
    .to_string();
    let payload = build_legacy_payload(METHOD, COMMS_PATH, &ts, &metadata);
    assert!(verify(
        &headers_for(&wallet, &payload, &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .is_ok());
}

/// The bypass the 6.3 guard closes: signed in the current format, the delivered
/// bytes are the signed bytes and the fallback's key guard never runs, so only
/// the gate can refuse a key that folds to `signer` without being spelled so.
#[tokio::test]
async fn refuses_a_folded_signer_key_signed_in_the_current_format() {
    for key in ["Signer", "SIGNER", "sIgNeR"] {
        let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
        let ts = now_ms();
        let metadata = json!({
            "origin": "https://play.decentraland.org",
            "intent": "dcl:explorer:comms-handshake",
            "isGuest": "false",
            key: KERNEL_SCENE_SIGNER,
        })
        .to_string();
        let payload = build_payload_v6(METHOD, COMMS_PATH, &ts, &metadata);
        let err = verify(
            &headers_for(&wallet, &payload, &ts, &metadata),
            COMMS_PATH,
            EXPLORER_METADATA_KEYS,
        )
        .await
        .expect_err("a key folding to signer must not read as having no signer");
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. })
                && err.http_message().contains("invalid metadata content"),
            "{key:?} produced {err:?}"
        );
    }
}

/// The exact key is present and canonical with the hostile spelling beside it.
/// Read on the exact key alone the request is served; the pair must not be
/// splittable into the one the gate reads and the one it ignores.
#[tokio::test]
async fn refuses_a_folded_duplicate_beside_a_canonical_signer() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata = json!({
        "origin": "https://play.decentraland.org",
        "intent": "dcl:explorer:comms-handshake",
        "signer": "dcl:explorer",
        "isGuest": "false",
        "Signer": KERNEL_SCENE_SIGNER,
    })
    .to_string();
    let payload = build_payload_v6(METHOD, COMMS_PATH, &ts, &metadata);
    let err = verify(
        &headers_for(&wallet, &payload, &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .expect_err("a folded duplicate must not be authorized on the exact key");
    assert!(err.http_message().contains("invalid metadata content"));
}

/// Padding and re-casing are signature-bound under the 6.x payload: the value
/// was that way when signed, so the request is authentic and only the gate can
/// refuse it. Upstream refuses a non-canonical `signer` rather than folding it.
#[tokio::test]
async fn refuses_the_scene_signer_under_every_spelling() {
    for spelling in [
        KERNEL_SCENE_SIGNER,
        "Decentraland-Kernel-Scene",
        " decentraland-kernel-scene",
        "decentraland-kernel-scene ",
        "\tDECENTRALAND-KERNEL-SCENE",
    ] {
        let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
        let ts = now_ms();
        let metadata = explorer_metadata(spelling);
        let payload = build_payload_v6(METHOD, COMMS_PATH, &ts, &metadata);
        let err = verify(
            &headers_for(&wallet, &payload, &ts, &metadata),
            COMMS_PATH,
            EXPLORER_METADATA_KEYS,
        )
        .await
        .expect_err("a scene-signed request must never mint a comms token");
        assert!(
            err.http_message().contains("invalid metadata content"),
            "signer {spelling:?} produced {err:?}"
        );
    }
}

/// The gate answers before signature verification, so a refused request pays
/// no catalyst round-trip for an EIP-1654 chain.
#[tokio::test]
async fn the_scene_gate_answers_before_signature_verification() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata = explorer_metadata(KERNEL_SCENE_SIGNER);
    let err = verify(
        &headers_for(&wallet, "not-the-payload", &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .expect_err("a scene-signed request must be refused");
    assert!(
        matches!(err, AuthChainError::MalformedChain { .. }),
        "the gate must win over the signature check, got {err:?}"
    );
}

/// Freshness is checked ahead of the gate, so a replayed handshake is a 401
/// rather than a 400 whatever its metadata says.
#[tokio::test]
async fn a_stale_handshake_is_refused_before_the_gate_runs() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = (chrono::Utc::now().timestamp_millis() - 10 * 60 * 1000).to_string();
    let metadata = explorer_metadata(KERNEL_SCENE_SIGNER);
    let payload = build_payload_v6(METHOD, COMMS_PATH, &ts, &metadata);
    let err = verify(
        &headers_for(&wallet, &payload, &ts, &metadata),
        COMMS_PATH,
        EXPLORER_METADATA_KEYS,
    )
    .await
    .expect_err("a ten-minute-old handshake must not be served");
    assert!(matches!(err, AuthChainError::Expired { .. }), "{err:?}");
}

/// creator-hub still signs the folded payload when it sets a world password or
/// an allow list, and everything it sends carries uppercase. The secret reaches
/// the handler exactly as delivered: nothing is folded on the way through.
#[tokio::test]
async fn accepts_the_folded_permissions_payload_creator_hub_signs() {
    for metadata in [
        json!({ "type": "shared-secret", "secret": "MySecret" }),
        json!({ "type": "allow-list", "wallets": ["0xAbC0000000000000000000000000000000000001"] }),
        json!({ "type": "allow-list", "wallets": [], "communities": ["Community-One"] }),
        json!({ "type": "nft-ownership", "nft": "urn:decentraland:matic:collections-v2:0xAbC" }),
    ] {
        let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
        let ts = now_ms();
        let raw = metadata.to_string();
        let payload = build_legacy_payload(METHOD, PERMISSIONS_PATH, &ts, &raw);
        let auth = verify(
            &headers_for(&wallet, &payload, &ts, &raw),
            PERMISSIONS_PATH,
            PERMISSIONS_METADATA_KEYS,
        )
        .await
        .unwrap_or_else(|err| panic!("{raw} must verify on the permissions route: {err:?}"));
        assert!(signed_by(&auth, &wallet));
        assert_eq!(auth.metadata, metadata);
        assert_eq!(
            auth.secret(),
            metadata["secret"].as_str().map(str::to_string)
        );
    }
}

/// Every field `post_permissions` reads is pinned, so a legacy request that
/// re-spells one is refused instead of being read as "not set".
#[tokio::test]
async fn refuses_a_legacy_permissions_request_that_respells_a_declared_key() {
    for (respelled, guard) in [
        ("Type", "expected \"type\""),
        ("Secret", "expected \"secret\""),
        ("Wallets", "expected \"wallets\""),
        ("Communities", "expected \"communities\""),
        ("Nft", "expected \"nft\""),
    ] {
        let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
        let ts = now_ms();
        let metadata = json!({ respelled: "shared-secret" }).to_string();
        let payload = build_legacy_payload(METHOD, PERMISSIONS_PATH, &ts, &metadata);
        let err = verify(
            &headers_for(&wallet, &payload, &ts, &metadata),
            PERMISSIONS_PATH,
            PERMISSIONS_METADATA_KEYS,
        )
        .await
        .expect_err("a re-spelled permissions key must not be served");
        assert!(
            err.http_message().contains(guard),
            "{respelled:?} produced {}",
            err.http_message()
        );
    }
}

/// The owner-only routes verify the 6.x payload alone, as upstream keeps them:
/// a folded signature over cased metadata is refused outright, a re-spelled
/// `signer` is still the gate's to refuse, and the `{}` ui3 and dcl-one-sdk
/// send folds to itself.
#[tokio::test]
async fn owner_routes_verify_the_current_format_only() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let metadata =
        json!({ "Wallets": ["0xAbC"], "origin": "https://catalyst.example.com" }).to_string();
    let payload = build_legacy_payload("put", SETTINGS_PATH, &ts, &metadata);
    let err = require_verified(
        &headers_for(&wallet, &payload, &ts, &metadata),
        "put",
        SETTINGS_PATH,
        &[],
    )
    .await
    .expect_err("a folded signature over cased metadata has no fallback here");
    assert!(
        matches!(err, AuthChainError::InvalidSignature(_)),
        "the strict route must answer 401, got {err:?}"
    );

    let metadata = json!({ "Signer": "dcl:explorer" }).to_string();
    let payload = build_legacy_payload("put", SETTINGS_PATH, &ts, &metadata);
    let err = require_verified(
        &headers_for(&wallet, &payload, &ts, &metadata),
        "put",
        SETTINGS_PATH,
        &[],
    )
    .await
    .expect_err("a re-spelled signer must be refused on every route");
    assert!(err.http_message().contains("invalid metadata content"));

    let payload = build_legacy_payload("put", SETTINGS_PATH, &ts, "{}");
    let auth = require_verified(
        &headers_for(&wallet, &payload, &ts, "{}"),
        "put",
        SETTINGS_PATH,
        &[],
    )
    .await
    .expect("the folded payload over {} is the 6.x payload");
    assert!(signed_by(&auth, &wallet));
}

/// ui3 sends `{}` as metadata, which folds to itself: the request verifies on
/// the first attempt and never reaches the fallback.
#[tokio::test]
async fn an_empty_metadata_request_verifies_on_the_current_format() {
    let wallet = Wallet::from_hex(EXPLORER_KEY).unwrap();
    let ts = now_ms();
    let payload = build_payload_v6("put", SETTINGS_PATH, &ts, "{}");
    assert_eq!(
        payload,
        build_legacy_payload("put", SETTINGS_PATH, &ts, "{}")
    );
    let auth = require_verified(
        &headers_for(&wallet, &payload, &ts, "{}"),
        "put",
        SETTINGS_PATH,
        &[],
    )
    .await
    .expect("an empty-metadata request must verify with no fallback declared");
    assert!(signed_by(&auth, &wallet));
}
