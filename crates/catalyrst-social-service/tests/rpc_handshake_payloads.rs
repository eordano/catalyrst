#![cfg(feature = "rpc")]

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_social_service::rpc::auth_chain::{
    require_signer, verify_handshake, AuthChainError, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, FIVE_MINUTES_SECS,
};

const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const METHOD: &str = "get";
const PATH: &str = "/";
const EXPLORER_METADATA: &str = "{}";
const CASED_METADATA: &str = r#"{"signer":"dcl:explorer","sceneId":"bafkreiAbC123"}"#;

#[derive(Clone, Copy, Debug)]
enum Shape {
    Legacy,
    V6,
}

const SHAPES: [Shape; 2] = [Shape::Legacy, Shape::V6];

fn wallet() -> Wallet {
    Wallet::from_hex(TEST_KEY).unwrap()
}

fn expected_signer() -> String {
    wallet().address().to_lowercase()
}

fn payload(shape: Shape, method: &str, path: &str, ts: &str, metadata: &str) -> String {
    match shape {
        Shape::Legacy => build_legacy_payload(method, path, ts, metadata),
        Shape::V6 => build_payload_v6(method, path, ts, metadata),
    }
}

fn frame_for(
    shape: Shape,
    signed_path: &str,
    signed_metadata: &str,
    delivered_metadata: &str,
    ts_ms: i64,
) -> serde_json::Map<String, serde_json::Value> {
    let ts = ts_ms.to_string();
    let signed = payload(shape, METHOD, signed_path, &ts, signed_metadata);
    let chain = create_simple_auth_chain(&wallet(), &signed).unwrap();

    let mut obj = serde_json::Map::new();
    obj.insert(AUTH_TIMESTAMP_HEADER.to_string(), ts.into());
    obj.insert(
        AUTH_METADATA_HEADER.to_string(),
        delivered_metadata.to_string().into(),
    );
    for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
        obj.insert(
            format!("{AUTH_CHAIN_HEADER_PREFIX}{i}"),
            link.to_string().into(),
        );
    }
    obj
}

fn frame(shape: Shape, signed_path: &str, metadata: &str, ts_ms: i64) -> String {
    serde_json::Value::Object(frame_for(shape, signed_path, metadata, metadata, ts_ms)).to_string()
}

fn headers(shape: Shape, signed_path: &str, metadata: &str, ts_ms: i64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in frame_for(shape, signed_path, metadata, metadata, ts_ms) {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value.as_str().unwrap()).unwrap(),
        );
    }
    headers
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[test]
fn the_explorer_handshake_metadata_folds_to_itself() {
    assert_eq!(
        build_legacy_payload(METHOD, PATH, "1", EXPLORER_METADATA),
        build_payload_v6(METHOD, PATH, "1", EXPLORER_METADATA)
    );
    assert_ne!(
        build_legacy_payload(METHOD, PATH, "1", CASED_METADATA),
        build_payload_v6(METHOD, PATH, "1", CASED_METADATA)
    );
}

#[tokio::test]
async fn the_explorer_handshake_verifies_under_either_signing_shape() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let f = frame(shape, PATH, EXPLORER_METADATA, ts_ms);
        let signer = verify_handshake(&f, METHOD, PATH, FIVE_MINUTES_SECS, ts_ms / 1000)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());

        let h = headers(shape, PATH, EXPLORER_METADATA, ts_ms);
        let signer = require_signer(&h, METHOD, PATH)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());
    }
}

#[tokio::test]
async fn the_socket_stays_6x_only_for_metadata_that_does_not_fold_to_itself() {
    let ts_ms = now_ms();
    let now = ts_ms / 1000;

    let v6 = frame(Shape::V6, PATH, CASED_METADATA, ts_ms);
    let signer = verify_handshake(&v6, METHOD, PATH, FIVE_MINUTES_SECS, now)
        .await
        .unwrap();
    assert_eq!(signer, expected_signer());

    let legacy = frame(Shape::Legacy, PATH, CASED_METADATA, ts_ms);
    let err = verify_handshake(&legacy, METHOD, PATH, FIVE_MINUTES_SECS, now)
        .await
        .unwrap_err();
    assert!(
        matches!(err, AuthChainError::InvalidSignature(_)),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_bad_signature_still_fails_as_invalid_signature_under_either_shape() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let f = frame(shape, "/social-rpc", EXPLORER_METADATA, ts_ms);
        let err = verify_handshake(&f, METHOD, PATH, FIVE_MINUTES_SECS, ts_ms / 1000)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );

        let h = headers(shape, "/social-rpc", EXPLORER_METADATA, ts_ms);
        let err = require_signer(&h, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );

        let stale = frame(shape, PATH, EXPLORER_METADATA, ts_ms);
        let err = verify_handshake(
            &stale,
            METHOD,
            PATH,
            FIVE_MINUTES_SECS,
            ts_ms / 1000 + FIVE_MINUTES_SECS + 60,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, AuthChainError::Expired { .. }),
            "{shape:?}: {err:?}"
        );
    }
}
