use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_scene_state::auth::{
    verify_auth_frame, AuthError, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, FIVE_MINUTES,
};

const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const METHOD: &str = "GET";
const PATH: &str = "/ws/MyWorld.dcl.eth";
const METADATA: &str =
    r#"{"realmName":"MyWorld.dcl.eth","sceneId":"bafkreiAbC123","parcel":"1,-2","isGuest":false}"#;

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

fn frame(
    shape: Shape,
    signed_path: &str,
    signed_metadata: &str,
    delivered_metadata: &str,
    ts_ms: i64,
) -> Vec<u8> {
    serde_json::Value::Object(frame_for(
        shape,
        signed_path,
        signed_metadata,
        delivered_metadata,
        ts_ms,
    ))
    .to_string()
    .into_bytes()
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

#[test]
fn either_shape_verifies_through_the_ws_auth_frame() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let body = frame(shape, PATH, METADATA, METADATA, ts_ms);
        let authed = verify_auth_frame(&body, METHOD, PATH, ts_ms / 1000)
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(authed.signer, expected_signer());
    }
}

#[test]
fn a_bad_signature_still_fails_as_invalid_signature_under_either_shape() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let now = ts_ms / 1000;

        let wrong_path = frame(shape, "/ws/Other.dcl.eth", METADATA, METADATA, ts_ms);
        let err = verify_auth_frame(&wrong_path, METHOD, PATH, now).unwrap_err();
        assert!(
            matches!(err, AuthError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );

        let tampered = METADATA.replace("bafkreiAbC123", "bafkreiXyZ999");
        assert_ne!(tampered, METADATA);
        let tampered_metadata = frame(shape, PATH, METADATA, &tampered, ts_ms);
        let err = verify_auth_frame(&tampered_metadata, METHOD, PATH, now).unwrap_err();
        assert!(
            matches!(err, AuthError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );
    }
}

#[test]
fn a_deterministic_failure_is_not_retried_against_the_other_shape() {
    for shape in SHAPES {
        let ts_ms = now_ms();
        let body = frame(shape, PATH, METADATA, METADATA, ts_ms);
        let err =
            verify_auth_frame(&body, METHOD, PATH, ts_ms / 1000 + FIVE_MINUTES + 60).unwrap_err();
        assert!(
            matches!(err, AuthError::Expired { .. }),
            "{shape:?}: {err:?}"
        );
    }
}
