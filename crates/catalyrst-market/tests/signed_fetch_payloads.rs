use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_crypto::sign::{create_simple_auth_chain, Wallet};
use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
use catalyrst_market::auth_chain::{
    extract_auth_chain, optional_signer, require_signer, validate_signature_either_payload,
    AuthChainError, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    FIVE_MINUTES,
};

const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
const METHOD: &str = "get";
const PATH: &str = "/v1/lists";
const METADATA: &str = r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick","sceneId":"bafkreiAbC123","isGuest":false}"#;

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

fn headers_for(
    shape: Shape,
    signed_path: &str,
    signed_metadata: &str,
    delivered_metadata: &str,
    ts_ms: i64,
) -> HeaderMap {
    let ts = ts_ms.to_string();
    let signed = payload(shape, METHOD, signed_path, &ts, signed_metadata);
    let chain = create_simple_auth_chain(&wallet(), &signed).unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_str(&ts).unwrap());
    headers.insert(
        AUTH_METADATA_HEADER,
        HeaderValue::from_str(delivered_metadata).unwrap(),
    );
    for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
        headers.insert(
            HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes()).unwrap(),
            HeaderValue::from_str(&link.to_string()).unwrap(),
        );
    }
    headers
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
async fn either_shape_verifies_through_require_signer_and_optional_signer() {
    for shape in SHAPES {
        let headers = headers_for(shape, PATH, METADATA, METADATA, now_ms());

        let signer = require_signer(&headers, METHOD, PATH)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());

        let signer = optional_signer(&headers, METHOD, PATH)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"))
            .unwrap_or_else(|| panic!("{shape:?}: optional_signer must recover a signer"));
        assert_eq!(signer, expected_signer());
    }
}

#[tokio::test]
async fn either_shape_verifies_with_a_proxy_prefixed_original_path() {
    for shape in SHAPES {
        let mut headers = headers_for(shape, "/market/v1/lists", METADATA, METADATA, now_ms());
        headers.insert(
            "x-original-path",
            HeaderValue::from_static("/market/v1/lists?page=1"),
        );
        let signer = require_signer(&headers, METHOD, PATH)
            .await
            .unwrap_or_else(|err| panic!("{shape:?}: {err:?}"));
        assert_eq!(signer, expected_signer());
    }
}

#[tokio::test]
async fn the_handler_verifier_accepts_both_shapes_over_the_extracted_chain() {
    for shape in SHAPES {
        for (method, path) in [("post", "/v1/trades"), ("get", "/v1/activity")] {
            let ts_ms = now_ms();
            let ts = ts_ms.to_string();
            let signed = payload(shape, method, path, &ts, METADATA);
            let chain = create_simple_auth_chain(&wallet(), &signed).unwrap();
            let mut headers = HeaderMap::new();
            for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
                headers.insert(
                    HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                        .unwrap(),
                    HeaderValue::from_str(&link.to_string()).unwrap(),
                );
            }
            let chain = extract_auth_chain(&headers).unwrap();

            let signer = validate_signature_either_payload(
                &chain,
                method,
                path,
                &ts,
                METADATA,
                FIVE_MINUTES,
                ts_ms / 1000,
            )
            .await
            .unwrap_or_else(|err| panic!("{shape:?} {method} {path}: {err:?}"));
            assert_eq!(signer, expected_signer());

            let err = validate_signature_either_payload(
                &chain,
                method,
                "/v1/other",
                &ts,
                METADATA,
                FIVE_MINUTES,
                ts_ms / 1000,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(err, AuthChainError::InvalidSignature(_)),
                "{shape:?} {method} {path}: {err:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_bad_signature_still_fails_as_invalid_signature_under_either_shape() {
    for shape in SHAPES {
        let now = now_ms();
        let wrong_path = headers_for(shape, "/v1/other", METADATA, METADATA, now);
        let err = require_signer(&wrong_path, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{shape:?}: {err:?}"
        );
        let err = optional_signer(&wrong_path, METHOD, PATH)
            .await
            .expect_err("a chain that does not verify must not read as anonymous");
        assert_eq!(err.to_string(), "Invalid Auth Chain", "{shape:?}");

        let tampered = METADATA.replace("bafkreiAbC123", "bafkreiXyZ999");
        assert_ne!(tampered, METADATA);
        let tampered_metadata = headers_for(shape, PATH, METADATA, &tampered, now);
        let err = require_signer(&tampered_metadata, METHOD, PATH)
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
    let stale = now_ms() - (FIVE_MINUTES + 60) * 1000;
    for shape in SHAPES {
        let headers = headers_for(shape, PATH, METADATA, METADATA, stale);
        let err = require_signer(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::Expired { .. }),
            "{shape:?}: {err:?}"
        );

        let mut headers = headers_for(shape, PATH, METADATA, METADATA, now_ms());
        headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_static("soon"));
        let err = require_signer(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidTimestamp(_)),
            "{shape:?}: {err:?}"
        );

        let mut headers = headers_for(shape, PATH, METADATA, METADATA, now_ms());
        headers.remove(AUTH_TIMESTAMP_HEADER);
        let err = require_signer(&headers, METHOD, PATH).await.unwrap_err();
        assert!(
            matches!(err, AuthChainError::MissingTimestamp),
            "{shape:?}: {err:?}"
        );
    }
}
