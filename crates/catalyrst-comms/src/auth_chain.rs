use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{field_is_canonical, Signer, SignerGate};
use catalyrst_types::AuthLinkType;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, header_str, signed_fetch_path, try_extract,
    validate_signature, AuthChain, AuthChainError, AuthLink, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;
pub const DEFAULT_EXPIRATION_SECS: i64 = 60;

#[derive(Debug)]
pub struct SignedFetchError {
    pub status: u16,
    pub message: String,
}

impl SignedFetchError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

pub struct SignedFetch {
    pub signer: Signer,
    pub metadata: serde_json::Value,
    pub is_guest: bool,
}

pub fn chain_is_guest(chain: &AuthChain) -> bool {
    !chain.links.iter().any(|link| {
        matches!(
            link.kind,
            AuthLinkType::EcdsaEphemeral | AuthLinkType::EcdsaEip1654Ephemeral
        )
    })
}

pub async fn verify_signed_fetch(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
) -> Result<SignedFetch, SignedFetchError> {
    verify_signed_fetch_gated(headers, method, path, allowed_signers, None).await
}

/// Both metadata gates run before any crypto, so a refusal is a 400 that
/// costs no signature recovery and no catalyst round-trip for an EIP-1654
/// chain. Keep them ahead of `validate_signature`.
pub async fn verify_signed_fetch_gated(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
    reject_signer: Option<&SignerGate>,
) -> Result<SignedFetch, SignedFetchError> {
    let path = signed_fetch_path(headers, path);
    let chain = extract_auth_chain(headers).map_err(map_chain_error)?;

    let raw_metadata = header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata: serde_json::Value = serde_json::from_str(raw_metadata).map_err(|_| {
        SignedFetchError::new(400, format!("Invalid chain metadata: \"{raw_metadata}\""))
    })?;

    if reject_signer.is_some_and(|gate| !gate.permits(&metadata)) {
        return Err(invalid_metadata(raw_metadata));
    }

    if !allowed_signers.is_empty() {
        if !field_is_canonical(&metadata, "signer") {
            return Err(invalid_metadata(raw_metadata));
        }
        let signer_field = metadata
            .get("signer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !allowed_signers.contains(&signer_field) {
            return Err(invalid_metadata(raw_metadata));
        }
    }

    let ts = header_str(headers, AUTH_TIMESTAMP_HEADER)
        .unwrap_or("0")
        .to_string();
    let now = chrono::Utc::now().timestamp();

    let signer = signed_fetch::validate_signature_either_payload(
        &chain,
        method,
        &path,
        &ts,
        raw_metadata,
        DEFAULT_EXPIRATION_SECS,
        now,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            error = ?e,
            %method,
            %path,
            %raw_metadata,
            signer = %chain.signer,
            "signed-fetch rejected"
        );
        match e {
            AuthChainError::Expired { .. } => SignedFetchError::new(401, "Expired signature"),
            AuthChainError::InvalidSignature(d) => {
                SignedFetchError::new(401, format!("Invalid signature: {d}"))
            }
            AuthChainError::EipNotImplemented => {
                SignedFetchError::new(503, "EIP-1654 validation unavailable")
            }
            other => SignedFetchError::new(400, other.to_string()),
        }
    })?;

    let is_guest = chain_is_guest(&chain);
    Ok(SignedFetch {
        signer,
        metadata,
        is_guest,
    })
}

fn invalid_metadata(raw_metadata: &str) -> SignedFetchError {
    SignedFetchError::new(400, format!("Invalid metadata content: {raw_metadata}"))
}

fn map_chain_error(e: AuthChainError) -> SignedFetchError {
    match e {
        AuthChainError::MalformedChain { detail } => {
            SignedFetchError::new(400, format!("Invalid chain format: {detail}"))
        }
        AuthChainError::InsufficientLinks => SignedFetchError::new(400, "Invalid Auth Chain"),
        other => SignedFetchError::new(400, other.to_string()),
    }
}

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    signed_fetch::try_extract_signer(headers, method, path, FIVE_MINUTES).await
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    signed_fetch::verify_signed_fetch(headers, method, path, FIVE_MINUTES).await
}

#[cfg(test)]
mod is_guest_tests {
    use super::*;

    fn link(kind: AuthLinkType) -> AuthLink {
        AuthLink {
            kind,
            payload: String::new(),
            signature: String::new(),
        }
    }

    fn chain(links: Vec<AuthLink>) -> AuthChain {
        AuthChain {
            links,
            signer: String::new(),
        }
    }

    #[test]
    fn guest_chain_without_ephemeral_is_guest() {
        assert!(chain_is_guest(&chain(vec![
            link(AuthLinkType::SIGNER),
            link(AuthLinkType::EcdsaSignedEntity),
        ])));
    }

    #[test]
    fn ephemeral_delegation_is_not_guest() {
        assert!(!chain_is_guest(&chain(vec![
            link(AuthLinkType::SIGNER),
            link(AuthLinkType::EcdsaEphemeral),
            link(AuthLinkType::EcdsaSignedEntity),
        ])));
    }

    #[test]
    fn eip1654_ephemeral_delegation_is_not_guest() {
        assert!(!chain_is_guest(&chain(vec![
            link(AuthLinkType::SIGNER),
            link(AuthLinkType::EcdsaEip1654Ephemeral),
            link(AuthLinkType::EcdsaEip1654SignedEntity),
        ])));
    }
}

#[cfg(test)]
mod signer_gate_tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use catalyrst_crypto::{create_simple_auth_chain, reject_if_signer, Wallet};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
    const SCENE: &str = "decentraland-kernel-scene";
    const PATH: &str = "/scene-admin";

    fn signed(metadata: &str) -> HeaderMap {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let payload = build_payload("post", PATH, &timestamp, metadata);
        let chain = create_simple_auth_chain(&wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
            headers.insert(
                HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                    .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            );
        }
        headers
    }

    fn status(result: Result<SignedFetch, SignedFetchError>) -> Result<String, (u16, String)> {
        result
            .map(|sf| sf.metadata.to_string())
            .map_err(|e| (e.status, e.message))
    }

    async fn allowlisted(metadata: &str) -> Result<String, (u16, String)> {
        status(verify_signed_fetch(&signed(metadata), "post", PATH, &[SCENE]).await)
    }

    #[tokio::test]
    async fn a_canonical_scene_signer_passes_the_allowlist() {
        let metadata = r#"{"signer":"decentraland-kernel-scene"}"#;
        assert_eq!(allowlisted(metadata).await, Ok(metadata.to_string()));
    }

    #[tokio::test]
    async fn a_folded_duplicate_beside_the_exact_signer_key_is_refused() {
        let metadata = r#"{"signer":"decentraland-kernel-scene","Signer":"x"}"#;
        assert_eq!(
            allowlisted(metadata).await,
            Err((400, format!("Invalid metadata content: {metadata}")))
        );
    }

    #[tokio::test]
    async fn a_respelled_signer_key_alone_is_refused() {
        let metadata = r#"{"Signer":"decentraland-kernel-scene"}"#;
        assert_eq!(
            allowlisted(metadata).await,
            Err((400, format!("Invalid metadata content: {metadata}")))
        );
    }

    #[tokio::test]
    async fn the_reject_gate_refuses_before_the_signature_is_checked() {
        let gate = reject_if_signer(&[SCENE]).unwrap();
        let mut headers = signed(r#"{"Signer":"decentraland-kernel-scene"}"#);
        headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_static("0"));
        let refused = verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate)).await;
        assert_eq!(status(refused).map_err(|(s, _)| s), Err(400));

        let mut headers = signed("{}");
        headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_static("0"));
        let late = verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate)).await;
        assert_eq!(status(late).map_err(|(s, _)| s), Err(401));
    }
}

#[cfg(test)]
mod payload_shape_tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
    use catalyrst_crypto::{create_simple_auth_chain, reject_if_signer, Wallet};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
    const SCENE: &str = "decentraland-kernel-scene";
    const PATH: &str = "/scene-bans";
    const METADATA: &str = r#"{"signer":"decentraland-kernel-scene","intent":"dcl:scene:ban","isGuest":false,"realmName":"LocalPreview","sceneId":"bafkreiAbC123"}"#;

    #[derive(Clone, Copy, Debug)]
    enum Shape {
        Legacy,
        V6,
    }

    const SHAPES: [Shape; 2] = [Shape::Legacy, Shape::V6];

    fn signed_as(shape: Shape, signed_path: &str, signed: &str, delivered: &str) -> HeaderMap {
        let wallet = Wallet::from_hex(KEY).unwrap();
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let payload = match shape {
            Shape::Legacy => build_legacy_payload("post", signed_path, &timestamp, signed),
            Shape::V6 => build_payload_v6("post", signed_path, &timestamp, signed),
        };
        let chain = create_simple_auth_chain(&wallet, &payload).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_TIMESTAMP_HEADER,
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(delivered).unwrap(),
        );
        for (i, link) in chain.as_array().into_iter().flatten().enumerate() {
            headers.insert(
                HeaderName::from_bytes(format!("{AUTH_CHAIN_HEADER_PREFIX}{i}").as_bytes())
                    .unwrap(),
                HeaderValue::from_str(&link.to_string()).unwrap(),
            );
        }
        headers
    }

    fn expected_signer() -> String {
        Wallet::from_hex(KEY).unwrap().address().to_lowercase()
    }

    fn failure(err: SignedFetchError) -> (u16, String) {
        (err.status, err.message)
    }

    #[test]
    fn the_fixture_metadata_makes_the_two_shapes_differ() {
        assert_ne!(
            build_legacy_payload("post", PATH, "1", METADATA),
            build_payload_v6("post", PATH, "1", METADATA)
        );
    }

    #[tokio::test]
    async fn either_shape_with_mixed_case_metadata_verifies_through_the_allowlist() {
        for shape in SHAPES {
            let headers = signed_as(shape, PATH, METADATA, METADATA);
            let sf = verify_signed_fetch(&headers, "post", PATH, &[SCENE])
                .await
                .unwrap_or_else(|e| panic!("{shape:?}: {} {}", e.status, e.message));
            assert_eq!(sf.signer, expected_signer());
            assert_eq!(sf.metadata["sceneId"], serde_json::json!("bafkreiAbC123"));
            assert_eq!(sf.metadata["realmName"], serde_json::json!("LocalPreview"));
            assert!(sf.is_guest);
        }
    }

    #[tokio::test]
    async fn either_shape_with_mixed_case_metadata_verifies_through_the_reject_gate() {
        let gate = reject_if_signer(&["dcl:explorer"]).unwrap();
        for shape in SHAPES {
            let headers = signed_as(shape, PATH, METADATA, METADATA);
            let sf = verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate))
                .await
                .unwrap_or_else(|e| panic!("{shape:?}: {} {}", e.status, e.message));
            assert_eq!(sf.signer, expected_signer());
            assert_eq!(sf.metadata["sceneId"], serde_json::json!("bafkreiAbC123"));
        }
    }

    #[tokio::test]
    async fn a_bad_signature_still_answers_401_invalid_signature_under_either_shape() {
        for shape in SHAPES {
            let wrong_path = signed_as(shape, "/scene-admin", METADATA, METADATA);
            let (status, message) = verify_signed_fetch(&wrong_path, "post", PATH, &[SCENE])
                .await
                .map(|_| ())
                .map_err(failure)
                .unwrap_err();
            assert_eq!(status, 401, "{shape:?}: {message}");
            assert!(
                message.starts_with("Invalid signature: "),
                "{shape:?}: {message}"
            );

            let tampered = METADATA.replace("bafkreiAbC123", "bafkreiXyZ999");
            assert_ne!(tampered, METADATA);
            let tampered_metadata = signed_as(shape, PATH, METADATA, &tampered);
            let (status, message) =
                verify_signed_fetch_gated(&tampered_metadata, "post", PATH, &[SCENE], None)
                    .await
                    .map(|_| ())
                    .map_err(failure)
                    .unwrap_err();
            assert_eq!(status, 401, "{shape:?}: {message}");
            assert!(
                message.starts_with("Invalid signature: "),
                "{shape:?}: {message}"
            );
        }
    }

    #[tokio::test]
    async fn the_gates_still_answer_400_before_either_signature_attempt() {
        let gate = reject_if_signer(&[SCENE]).unwrap();
        for shape in SHAPES {
            let headers = signed_as(shape, PATH, METADATA, METADATA);
            let (status, message) =
                verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate))
                    .await
                    .map(|_| ())
                    .map_err(failure)
                    .unwrap_err();
            assert_eq!(
                (status, message),
                (400, format!("Invalid metadata content: {METADATA}")),
                "{shape:?}"
            );

            let recased = METADATA.replace("\"signer\"", "\"Signer\"");
            let headers = signed_as(shape, PATH, &recased, &recased);
            let (status, _) = verify_signed_fetch(&headers, "post", PATH, &[SCENE])
                .await
                .map(|_| ())
                .map_err(failure)
                .unwrap_err();
            assert_eq!(status, 400, "{shape:?}");
        }
    }
}
