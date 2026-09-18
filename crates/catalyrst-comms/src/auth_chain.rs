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

/// The metadata fields a legacy-payload request may still be authorized on, as
/// upstream comms-gatekeeper declares them (`src/logic/utils.ts`
/// `CANONICAL_METADATA_KEYS`). The pre-6.0.0 fold left key casing outside the
/// signature, so a request re-spelled after signing still verifies; naming the
/// fields is what makes accepting that shape safe, and the guard refuses the
/// request rather than folding it.
pub const CANONICAL_METADATA_KEYS: &[&str] = &[
    "signer",
    "intent",
    "sceneId",
    "parcel",
    "realmName",
    "deviceIdentifier",
    "realm.hostname",
    "realm.serverName",
];

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

/// Upstream's `requireSigner` middlewares (`auth`, `authSceneOrServer`,
/// `authExplorer` in comms-gatekeeper `src/controllers/routes.ts`), which are the
/// ones that declare `canonicalMetadataKeys`: the scene runtimes and the
/// authoritative server that reach these routes ship separately from this
/// service, so the folded payload stays acceptable behind the key guard.
pub async fn verify_signed_fetch(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
) -> Result<SignedFetch, SignedFetchError> {
    verify_with_keys(
        headers,
        method,
        path,
        allowed_signers,
        None,
        CANONICAL_METADATA_KEYS,
    )
    .await
}

/// Upstream's `rejectIfSigner` middlewares (`authWatcher` and the user-moderation
/// `signedFetch`), which deliberately declare no `canonicalMetadataKeys`: the cast
/// web app and the moderation dapps sign all-lowercase metadata, so both payload
/// shapes are byte-identical for them and a fallback would only widen the accept
/// set past upstream's.
pub async fn verify_signed_fetch_gated(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
    reject_signer: Option<&SignerGate>,
) -> Result<SignedFetch, SignedFetchError> {
    verify_with_keys(headers, method, path, allowed_signers, reject_signer, &[]).await
}

/// Both metadata gates run before any crypto, so a refusal is a 400 that
/// costs no signature recovery and no catalyst round-trip for an EIP-1654
/// chain. Keep them ahead of the shared verifier.
async fn verify_with_keys(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    allowed_signers: &[&str],
    reject_signer: Option<&SignerGate>,
    canonical_metadata_keys: &[&str],
) -> Result<SignedFetch, SignedFetchError> {
    let chain = extract_auth_chain(headers).map_err(map_chain_error)?;

    let raw_metadata = header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata = metadata_object(raw_metadata)?;

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

    let signer = signed_fetch::verify_signed_fetch_with_legacy_fallback(
        headers,
        method,
        path,
        DEFAULT_EXPIRATION_SECS,
        canonical_metadata_keys,
        None,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            reason = auth_error_class(&e),
            %method,
            "signed-fetch rejected"
        );
        match e {
            AuthChainError::Expired { .. } => SignedFetchError::new(401, "Expired signature"),
            AuthChainError::InvalidSignature(_) => SignedFetchError::new(401, "Invalid signature"),
            AuthChainError::EipNotImplemented => {
                SignedFetchError::new(503, "EIP-1654 validation unavailable")
            }
            other => SignedFetchError::new(400, public_auth_error(&other)),
        }
    })?;

    let is_guest = chain_is_guest(&chain);
    Ok(SignedFetch {
        signer,
        metadata,
        is_guest,
    })
}

fn invalid_metadata(_raw_metadata: &str) -> SignedFetchError {
    SignedFetchError::new(400, "Invalid metadata content")
}

/// Upstream's `verifyMetadata` refuses an unparseable or non-object metadata
/// header outright and reads an explicit JSON `null` as an empty object. Both
/// gates below read fields off this value, so coercing anything else would let
/// them pass vacuously over a delivery upstream drops.
fn metadata_object(raw_metadata: &str) -> Result<serde_json::Value, SignedFetchError> {
    let refused = || SignedFetchError::new(400, "Invalid chain metadata");
    match serde_json::from_str::<serde_json::Value>(raw_metadata) {
        Ok(serde_json::Value::Null) => Ok(serde_json::Value::Object(serde_json::Map::new())),
        Ok(value @ serde_json::Value::Object(_)) => Ok(value),
        _ => Err(refused()),
    }
}

fn map_chain_error(e: AuthChainError) -> SignedFetchError {
    match e {
        AuthChainError::MalformedChain { .. } => SignedFetchError::new(400, "Invalid chain format"),
        AuthChainError::InsufficientLinks => SignedFetchError::new(400, "Invalid Auth Chain"),
        other => SignedFetchError::new(400, public_auth_error(&other)),
    }
}

fn auth_error_class(error: &AuthChainError) -> &'static str {
    match error {
        AuthChainError::MalformedChain { .. } => "malformed_chain",
        AuthChainError::InsufficientLinks => "insufficient_links",
        AuthChainError::MissingTimestamp => "missing_timestamp",
        AuthChainError::Expired { .. } => "expired",
        AuthChainError::InvalidSignature(_) => "invalid_signature",
        AuthChainError::ForbiddenSigner => "forbidden_signer",
        AuthChainError::EipNotImplemented => "eip1654_unavailable",
        AuthChainError::AddressMismatch { .. } => "address_mismatch",
        AuthChainError::InvalidTimestamp(_) => "invalid_timestamp",
        AuthChainError::CatalystUnavailable(_) => "catalyst_unavailable",
        AuthChainError::SceneSignerRejected => "scene_signer_rejected",
    }
}

fn public_auth_error(error: &AuthChainError) -> &'static str {
    match error {
        AuthChainError::MalformedChain { .. } => "Invalid chain format",
        AuthChainError::InsufficientLinks => "Invalid Auth Chain",
        AuthChainError::MissingTimestamp => "Missing timestamp",
        AuthChainError::Expired { .. } => "Expired signature",
        AuthChainError::InvalidSignature(_) => "Invalid signature",
        AuthChainError::ForbiddenSigner => "Access denied, invalid signer",
        AuthChainError::EipNotImplemented => "EIP-1654 validation unavailable",
        AuthChainError::AddressMismatch { .. } => "Forbidden: address mismatch",
        AuthChainError::InvalidTimestamp(_) => "Invalid timestamp",
        AuthChainError::CatalystUnavailable(_) => "Error connecting to catalyst",
        AuthChainError::SceneSignerRejected => "Requests from scenes are not allowed",
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
            Err((400, "Invalid metadata content".into()))
        );
    }

    #[tokio::test]
    async fn a_respelled_signer_key_alone_is_refused() {
        let metadata = r#"{"Signer":"decentraland-kernel-scene"}"#;
        assert_eq!(
            allowlisted(metadata).await,
            Err((400, "Invalid metadata content".into()))
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
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f2e3e8a5d4b8e3e3e3";
    const SCENE: &str = "decentraland-kernel-scene";
    const PATH: &str = "/scene-bans";
    const METADATA: &str = r#"{"signer":"decentraland-kernel-scene","intent":"dcl:scene:ban","isGuest":false,"realmName":"LocalPreview","sceneId":"bafkreiAbC123"}"#;
    const CREDENTIAL_CANARY: &str = "server-signed-metadata-private-canary";

    #[derive(Clone, Default)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for LogBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

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

    /// Upstream's `rejectIfSigner` routes declare no `canonicalMetadataKeys`, and
    /// core-libs `verify()` never attempts the legacy payload without them, so this
    /// path is 6.x-only: the folded shape is refused the way a signature mismatch is.
    #[tokio::test]
    async fn only_the_v6_shape_verifies_through_the_reject_gate() {
        let gate = reject_if_signer(&["dcl:explorer"]).unwrap();

        let headers = signed_as(Shape::V6, PATH, METADATA, METADATA);
        let sf = verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate))
            .await
            .unwrap_or_else(|e| panic!("{} {}", e.status, e.message));
        assert_eq!(sf.signer, expected_signer());
        assert_eq!(sf.metadata["sceneId"], serde_json::json!("bafkreiAbC123"));

        let headers = signed_as(Shape::Legacy, PATH, METADATA, METADATA);
        let (status, message) = verify_signed_fetch_gated(&headers, "post", PATH, &[], Some(&gate))
            .await
            .map(|_| ())
            .map_err(failure)
            .unwrap_err();
        assert_eq!(status, 401, "{message}");
        assert_eq!(message, "Invalid signature");
    }

    /// The same metadata on a `requireSigner` route, where upstream does declare the
    /// keys: both shapes verify, and the guard is what keeps the folded one honest.
    #[tokio::test]
    async fn the_declared_keys_are_the_ones_upstream_names() {
        assert_eq!(
            CANONICAL_METADATA_KEYS,
            &[
                "signer",
                "intent",
                "sceneId",
                "parcel",
                "realmName",
                "deviceIdentifier",
                "realm.hostname",
                "realm.serverName",
            ]
        );
    }

    /// `Number(raw || '0')` in core-libs `verifyTimestamp` reads a present-but-empty
    /// header as timestamp zero, so it answers the expiration window's 401 rather
    /// than the malformed-timestamp 400 an unguarded coercion produced.
    #[tokio::test]
    async fn a_present_but_empty_timestamp_expires_instead_of_reading_as_malformed() {
        let mut headers = signed_as(Shape::V6, PATH, METADATA, METADATA);
        headers.insert(AUTH_TIMESTAMP_HEADER, HeaderValue::from_static(""));
        let (status, message) = verify_signed_fetch(&headers, "post", PATH, &[SCENE])
            .await
            .map(|_| ())
            .map_err(failure)
            .unwrap_err();
        assert_eq!((status, message), (401, "Expired signature".to_string()));
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
            assert_eq!(message, "Invalid signature", "{shape:?}");

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
            assert_eq!(message, "Invalid signature", "{shape:?}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signed_fetch_failures_never_log_or_echo_metadata_and_signature_details() {
        assert_eq!(
            auth_error_class(&AuthChainError::InvalidSignature(CREDENTIAL_CANARY.into())),
            "invalid_signature"
        );
        let tampered = METADATA.replace("bafkreiAbC123", CREDENTIAL_CANARY);
        let headers = signed_as(Shape::V6, PATH, METADATA, &tampered);
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let error = match verify_signed_fetch(&headers, "post", PATH, &[SCENE]).await {
            Ok(_) => panic!("tampered metadata verified"),
            Err(error) => error,
        };
        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert_eq!(
            (error.status, error.message.as_str()),
            (401, "Invalid signature")
        );
        assert!(!logs.contains(CREDENTIAL_CANARY));

        let malformed = format!(r#"{{"private":"{CREDENTIAL_CANARY}""#);
        let error = match verify_signed_fetch(
            &signed_as(Shape::V6, PATH, &malformed, &malformed),
            "post",
            PATH,
            &[SCENE],
        )
        .await
        {
            Ok(_) => panic!("malformed metadata verified"),
            Err(error) => error,
        };
        assert_eq!(
            (error.status, error.message.as_str()),
            (400, "Invalid chain metadata")
        );
        assert!(!error.message.contains(CREDENTIAL_CANARY));
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
                (400, "Invalid metadata content".into()),
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
