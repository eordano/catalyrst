use alloy::signers::{local::PrivateKeySigner, Signer};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::response::IntoResponse;
use catalyrst_builder::auth_chain::build_payload;
use catalyrst_builder::handlers::curation::authorize_admin;

const CURATION_PATH: &str = "/v1/collections/curation";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn link_json(kind: &str, payload: &str, signature: &str) -> String {
    serde_json::json!({ "type": kind, "payload": payload, "signature": signature }).to_string()
}

async fn signed_headers(ts_ms: i64) -> (HeaderMap, String) {
    let root: PrivateKeySigner = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let root_addr = format!("{:#x}", root.address());

    let ephemeral: PrivateKeySigner =
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
            .parse()
            .unwrap();
    let ephemeral_addr = format!("{:#x}", ephemeral.address());

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = root
        .sign_message(ephemeral_payload.as_bytes())
        .await
        .unwrap()
        .to_string();

    let canonical = build_payload("get", CURATION_PATH, &ts_ms.to_string(), "{}");
    let entity_sig = ephemeral
        .sign_message(canonical.as_bytes())
        .await
        .unwrap()
        .to_string();

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_str(&link_json("SIGNER", &root_addr, "")).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-1"),
        HeaderValue::from_str(&link_json(
            "ECDSA_EPHEMERAL",
            &ephemeral_payload,
            &ephemeral_sig,
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json("ECDSA_SIGNED_ENTITY", &canonical, &entity_sig)).unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-timestamp"),
        HeaderValue::from_str(&ts_ms.to_string()).unwrap(),
    );

    (headers, root_addr.to_lowercase())
}

fn is_forbidden(err: catalyrst_builder::http::errors::ApiError) -> bool {
    err.into_response().status() == axum::http::StatusCode::FORBIDDEN
}

#[tokio::test]
async fn committee_address_signed_fetch_is_authorized() {
    let ts = now_ms();
    let (headers, signer) = signed_headers(ts).await;
    let admins = vec![signer.clone()];

    let res = authorize_admin(None, &admins, &headers, "get", CURATION_PATH);
    assert!(
        res.is_ok(),
        "allowlisted signer should be authorized: {res:?}"
    );
}

#[tokio::test]
async fn valid_signature_from_non_committee_address_is_forbidden() {
    let ts = now_ms();
    let (headers, signer) = signed_headers(ts).await;
    let admins = vec!["0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()];
    assert_ne!(signer, admins[0]);

    let err = authorize_admin(None, &admins, &headers, "get", CURATION_PATH)
        .expect_err("non-committee signer must be denied");
    assert!(is_forbidden(err));
}

#[tokio::test]
async fn empty_admin_token_does_not_weaken_the_signed_fetch_branch() {
    let ts = now_ms();
    let (headers, signer) = signed_headers(ts).await;

    assert!(authorize_admin(Some(""), &[signer], &headers, "get", CURATION_PATH).is_ok());

    let (headers2, _) = signed_headers(now_ms()).await;
    let err = authorize_admin(Some(""), &[], &headers2, "get", CURATION_PATH)
        .expect_err("empty token + empty allowlist must deny");
    assert!(is_forbidden(err));
}

#[tokio::test]
async fn expired_signed_fetch_is_forbidden() {
    let ts = now_ms() - 31 * 60 * 1000;
    let (headers, signer) = signed_headers(ts).await;
    let admins = vec![signer];

    let err = authorize_admin(None, &admins, &headers, "get", CURATION_PATH)
        .expect_err("stale timestamp must be denied");
    assert!(is_forbidden(err));
}

#[tokio::test]
async fn signature_bound_to_a_different_path_is_forbidden() {
    let ts = now_ms();
    let (headers, signer) = signed_headers(ts).await;
    let admins = vec![signer];

    let err = authorize_admin(None, &admins, &headers, "get", "/v1/collections/other")
        .expect_err("path-bound signature must not authorize a different path");
    assert!(is_forbidden(err));
}

#[tokio::test]
async fn malformed_auth_chain_headers_are_forbidden_not_panicking() {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_static("{not valid json"),
    );
    headers.insert(
        HeaderName::from_static("x-identity-timestamp"),
        HeaderValue::from_static("1735689600000"),
    );
    let err = authorize_admin(None, &[], &headers, "get", CURATION_PATH)
        .expect_err("garbage chain must be denied");
    assert!(is_forbidden(err));
}
