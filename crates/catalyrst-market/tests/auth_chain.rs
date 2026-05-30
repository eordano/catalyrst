use axum::http::{HeaderMap, HeaderName, HeaderValue};
use catalyrst_market::auth_chain::{
    build_payload, extract_auth_chain, validate_signature, verify_with_address, AuthChainError,
    FIVE_MINUTES,
};
use ethers_signers::{LocalWallet, Signer};

fn link_json(kind: &str, payload: &str, signature: &str) -> String {
    serde_json::json!({
        "type": kind,
        "payload": payload,
        "signature": signature,
    })
    .to_string()
}

async fn build_signed_chain(canonical_payload: &str) -> (HeaderMap, String) {
    let root: LocalWallet = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let root_addr = format!("{:#x}", root.address());

    let ephemeral: LocalWallet =
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
            .parse()
            .unwrap();
    let ephemeral_addr = format!("{:#x}", ephemeral.address());

    let ephemeral_payload = format!(
        "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
        ephemeral_addr
    );
    let ephemeral_sig = root.sign_message(ephemeral_payload.as_bytes()).await.unwrap();
    let ephemeral_sig_hex = format!("0x{}", ephemeral_sig);

    let entity_sig = ephemeral
        .sign_message(canonical_payload.as_bytes())
        .await
        .unwrap();
    let entity_sig_hex = format!("0x{}", entity_sig);

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
            &ephemeral_sig_hex,
        ))
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&link_json(
            "ECDSA_SIGNED_ENTITY",
            canonical_payload,
            &entity_sig_hex,
        ))
        .unwrap(),
    );

    (headers, root_addr)
}

#[tokio::test]
async fn valid_chain_correct_signer_and_matching_address_returns_ok() {
    let ts_ms = 1_735_689_600_000_i64;
    let payload = build_payload("get", "/v1/activity", &ts_ms.to_string(), "{}");
    let (headers, root_addr) = build_signed_chain(&payload).await;

    let chain = extract_auth_chain(&headers).expect("chain should parse");
    let recovered = verify_with_address(
        &chain,
        &payload,
        FIVE_MINUTES,
        ts_ms / 1000,
        &root_addr,
    )
    .expect("verification should succeed");

    assert_eq!(recovered.to_lowercase(), root_addr.to_lowercase());
}

#[tokio::test]
async fn valid_chain_but_mismatched_query_address_returns_forbidden() {
    let ts_ms = 1_735_689_600_000_i64;
    let payload = build_payload("get", "/v1/activity", &ts_ms.to_string(), "{}");
    let (headers, _root_addr) = build_signed_chain(&payload).await;

    let chain = extract_auth_chain(&headers).expect("chain should parse");

    let other = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let err = verify_with_address(&chain, &payload, FIVE_MINUTES, ts_ms / 1000, other)
        .expect_err("should reject on address mismatch");

    assert!(
        matches!(err, AuthChainError::AddressMismatch { .. }),
        "expected AddressMismatch, got {:?}",
        err
    );
    assert_eq!(err.message(), "Forbidden: address mismatch");
}

#[tokio::test]
async fn expired_timestamp_returns_err() {
    let signed_at_ms = 1_735_689_600_000_i64;
    let now_secs = signed_at_ms / 1000 + 600;

    let payload = build_payload("get", "/v1/activity", &signed_at_ms.to_string(), "{}");
    let (headers, root_addr) = build_signed_chain(&payload).await;
    let chain = extract_auth_chain(&headers).expect("chain should parse");

    let err = verify_with_address(&chain, &payload, FIVE_MINUTES, now_secs, &root_addr)
        .expect_err("should reject on expiration");

    assert!(
        matches!(err, AuthChainError::Expired { .. }),
        "expected Expired, got {:?}",
        err
    );
}

#[tokio::test]
async fn tampered_signature_returns_err() {
    let ts_ms = 1_735_689_600_000_i64;
    let payload = build_payload("get", "/v1/activity", &ts_ms.to_string(), "{}");
    let (mut headers, root_addr) = build_signed_chain(&payload).await;

    let raw_leaf = headers
        .get("x-identity-auth-chain-2")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let mut parsed: serde_json::Value = serde_json::from_str(&raw_leaf).unwrap();
    {
        let sig = parsed["signature"].as_str().unwrap().to_string();

        let mut chars: Vec<char> = sig.chars().collect();
        let idx = 2;
        let new_c = match chars[idx] {
            '0' => '1',
            _ => '0',
        };
        chars[idx] = new_c;
        let new_sig: String = chars.into_iter().collect();
        assert_ne!(new_sig, sig, "byte flip should have changed the signature");
        parsed["signature"] = serde_json::Value::String(new_sig);
    }
    let tampered = parsed.to_string();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-2"),
        HeaderValue::from_str(&tampered).unwrap(),
    );

    let chain = extract_auth_chain(&headers).expect("chain should parse (tampered sig still valid hex)");
    let err = validate_signature(&chain, &payload, FIVE_MINUTES, ts_ms / 1000)
        .expect_err("should reject tampered signature");

    assert!(
        matches!(err, AuthChainError::InvalidSignature(_)),
        "expected InvalidSignature, got {:?}",
        err
    );

    let bad_with_addr =
        verify_with_address(&chain, &payload, FIVE_MINUTES, ts_ms / 1000, &root_addr);
    assert!(bad_with_addr.is_err());
}

#[test]
fn missing_required_header_returns_malformed_chain() {
    let headers = HeaderMap::new();
    let err = extract_auth_chain(&headers).expect_err("empty headers must reject");
    assert!(
        matches!(err, AuthChainError::InsufficientLinks),
        "expected InsufficientLinks, got {:?}",
        err
    );
    assert_eq!(err.message(), "Invalid Auth Chain");

    let mut bad = HeaderMap::new();
    bad.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_static("{not json"),
    );
    let err2 = extract_auth_chain(&bad).expect_err("malformed json must reject");
    assert!(
        matches!(err2, AuthChainError::MalformedChain { .. }),
        "expected MalformedChain, got {:?}",
        err2
    );
    assert_eq!(err2.message(), "Invalid Auth Chain");
}

#[test]
fn single_link_chain_returns_insufficient_links() {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-identity-auth-chain-0"),
        HeaderValue::from_str(&link_json(
            "SIGNER",
            "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266",
            "",
        ))
        .unwrap(),
    );
    let err = extract_auth_chain(&headers).expect_err("1-link chain must reject");
    assert!(
        matches!(err, AuthChainError::InsufficientLinks),
        "expected InsufficientLinks, got {:?}",
        err
    );
}
