use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_quests::auth_chain::{
    optional_signer, require_signer, verify_handshake, AuthChainError, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, FIVE_MINUTES_SECS,
};

const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const METHOD: &str = "get";
const PATH: &str = "/api/quests/abc";
const METADATA: &str = r#"{"intent":"dcl:explorer:comms-handshake","signer":"dcl:explorer","isGuest":false,"realmName":"LocalPreview","sceneId":"bafkreiAbC123"}"#;

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
    method: &str,
    signed_path: &str,
    signed_metadata: &str,
    delivered_metadata: &str,
    ts_ms: i64,
) -> serde_json::Map<String, serde_json::Value> {
    let ts = ts_ms.to_string();
    let signed = payload(shape, method, signed_path, &ts, signed_metadata);
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

fn headers_for(
    shape: Shape,
    signed_path: &str,
    signed_metadata: &str,
    delivered_metadata: &str,
    ts_ms: i64,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in frame_for(
        shape,
        METHOD,
        signed_path,
        signed_metadata,
        delivered_metadata,
        ts_ms,
    ) {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value.as_str().unwrap()).unwrap(),
        );
    }
    headers
}

fn ws_frame(shape: Shape, signed_path: &str, ts_ms: i64) -> String {
    serde_json::Value::Object(frame_for(
        shape,
        "get",
        signed_path,
        METADATA,
        METADATA,
        ts_ms,
    ))
    .to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[test]
fn the_fixture_metadata_makes_the_two_shapes_differ() {
    assert_ne!(
        build_legacy_payload(METHOD, PATH, "1", METADATA),
        build_payload_v6(METHOD, PATH, "1", METADATA)
    );
}

#[tokio::test]
async fn either_shape_verifies_through_the_http_and_ws_verifiers() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let headers = headers_for(shape, PATH, METADATA, METADATA, ts_ms);

        let signer = require_signer(&headers, METHOD, PATH)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());

        let signer = optional_signer(&headers, METHOD, PATH)
            .await
            .unwrap_or_else(|| panic!("{shape:?}: optional_signer must recover a signer"));
        assert_eq!(signer, expected_signer());

        let frame = ws_frame(shape, "/", ts_ms);
        let signer = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, ts_ms / 1000)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());
    }
}

#[tokio::test]
async fn a_bad_signature_still_fails_as_invalid_signature_under_either_shape() {
    for shape in SHAPES {
        let ts_ms = now_ms();

        let wrong_path = headers_for(shape, "/api/quests/other", METADATA, METADATA, ts_ms);
        let err = require_signer(&wrong_path, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );
        assert!(optional_signer(&wrong_path, METHOD, PATH).await.is_none());

        let tampered = METADATA.replace("bafkreiAbC123", "bafkreiXyZ999");
        assert_ne!(tampered, METADATA);
        let tampered_metadata = headers_for(shape, PATH, METADATA, &tampered, ts_ms);
        let err = require_signer(&tampered_metadata, METHOD, PATH)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );

        let frame = ws_frame(shape, "/social-rpc", ts_ms);
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, ts_ms / 1000)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );
    }
}

#[tokio::test]
async fn a_deterministic_failure_is_not_retried_against_the_other_shape() {
    for shape in SHAPES {
        let stale = now_ms() - (FIVE_MINUTES_SECS + 60) * 1000;
        let headers = headers_for(shape, PATH, METADATA, METADATA, stale);
        let err = require_signer(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::Expired { .. }),
            "{shape:?}: {err:?}"
        );

        let ts_ms = now_ms();
        let frame = ws_frame(shape, "/", ts_ms);
        let err = verify_handshake(
            &frame,
            "get",
            "/",
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
