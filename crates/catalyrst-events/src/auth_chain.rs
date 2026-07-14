use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::eip1654::Eip1654Validator;
use catalyrst_crypto::metadata_gate::{
    assert_canonical_metadata_keys, assert_legacy_metadata_keys,
};
use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{reject_if_signer, Signer, SignerGate};

use crate::http::response::ApiError;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, extract_auth_chain, try_extract, validate_signature, AuthChain, AuthChainError,
    AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER,
    MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// The metadata fields this crate authorizes on: `signer` drives the ADR-44
/// no-scenes gate, `intent` drives the canonical-value gate. Naming them is
/// what lets the legacy payload stay acceptable: the pre-6.0.0 fold left key
/// casing outside the signature, so `{"Signer":"decentraland-kernel-scene"}`
/// shares a valid signature with the canonical spelling while every gate below
/// reads it as absent. Nothing else events reads out of the metadata (the
/// attendee display `name`) decides authorization, so a re-spelling there is
/// not a bypass and must not cost legacy clients their request.
const CANONICAL_METADATA_KEYS: &[&str] = &["signer", "intent"];

/// Upstream's `rejectIfSigner` gate, on the signers this service turns away.
/// The two crate-local gates below answer the string spellings with upstream's
/// 400s but reach for `as_str()` first, so a `signer` that is not a string
/// slips past both; this refuses it, as upstream does.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| {
        reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical by construction")
    })
}

/// The first 64 characters of the metadata, as delivered: enough to name which
/// header was refused without echoing an unbounded client string back.
fn echo(metadata: &str) -> String {
    metadata.chars().take(64).collect()
}

/// Unparseable, or parseable but not an object: refused before any signature
/// work, exactly as the crypto crate's own fallback verifier refuses it. The
/// legacy key guard is only meaningful over an object, and an explicit JSON
/// `null` reads as an empty one.
fn parse_metadata(metadata: &str) -> Result<serde_json::Value, AuthChainError> {
    match serde_json::from_str::<serde_json::Value>(metadata) {
        Ok(serde_json::Value::Null) => Ok(serde_json::Value::Object(serde_json::Map::new())),
        Ok(value @ serde_json::Value::Object(_)) => Ok(value),
        _ => Err(AuthChainError::MalformedChain {
            detail: format!("invalid chain metadata: \"{}\"", echo(metadata)),
        }),
    }
}

/// The verification step every route in this crate shares, staged the way the
/// crypto crate's fallback verifier stages it -- chain, timestamp, metadata
/// parse, signer gate, signature -- with the two payload shapes tried in the
/// opposite order.
///
/// Legacy first is a cost decision, not a security one. Every in-tree signer
/// (ui3, the wasm explorer, the scene runtime) still mints the pre-6.0.0 folded
/// payload and cannot be shipped ahead of this server, so trying 6.x first made
/// the ordinary request fail one signature check before passing the second:
/// a wasted local recovery on an ECDSA chain, and a wasted catalyst round-trip
/// on every authenticated EIP-1654 request. Flip the order back once the fleet
/// signs 6.x.
///
/// Neither attempt is reachable without the guard that makes it safe.
/// [`assert_legacy_metadata_keys`] gates the legacy payload exactly as it did
/// when the legacy payload was second, so a metadata key delivered in a
/// spelling other than the one [`CANONICAL_METADATA_KEYS`] authorizes on never
/// gets its folded signature validated at all; the 6.x attempt needs no such
/// guard because it signs the metadata bytes. Both attempts collapse into one
/// whenever the payloads are byte-identical, which is every request whose
/// metadata is already folded -- `{}` included, the shape ui3 sends.
///
/// Only `InvalidSignature` crosses from one attempt to the other: a malformed
/// chain, a missing or unparseable timestamp and an expired window are decided
/// identically for both shapes, so they answer without a second attempt.
async fn verify_with(
    headers: &HeaderMap,
    method: &str,
    path: &str,
    validator: Option<&dyn Eip1654Validator>,
) -> Result<Signer, AuthChainError> {
    assert_canonical_metadata_keys(CANONICAL_METADATA_KEYS)?;
    let path = signed_fetch::signed_fetch_path(headers, path);
    let path = path.as_ref();
    let chain = extract_auth_chain(headers)?;
    let timestamp = signed_fetch::header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    let raw = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata = parse_metadata(raw)?;

    if !scene_signer_gate().permits(&metadata) {
        return Err(AuthChainError::MalformedChain {
            detail: format!("invalid metadata content: \"{}\"", echo(raw)),
        });
    }

    let now = chrono::Utc::now().timestamp();
    let legacy = signed_fetch::build_legacy_payload(method, path, &timestamp, raw);
    let v6 = signed_fetch::build_payload_v6(method, path, &timestamp, raw);

    match assert_legacy_metadata_keys(&metadata, CANONICAL_METADATA_KEYS) {
        Ok(()) => {
            let attempt = signed_fetch::validate_signature_with(
                &chain,
                &legacy,
                &timestamp,
                FIVE_MINUTES,
                now,
                validator,
            )
            .await;
            match attempt {
                Err(AuthChainError::InvalidSignature(_)) if legacy != v6 => {}
                settled => return settled,
            }
        }
        Err(refused) if legacy == v6 => return Err(refused),
        Err(_) => {}
    }

    signed_fetch::validate_signature_with(&chain, &v6, &timestamp, FIVE_MINUTES, now, validator)
        .await
}

async fn verify(headers: &HeaderMap, method: &str, path: &str) -> Result<Signer, AuthChainError> {
    verify_with(
        headers,
        method,
        path,
        signed_fetch::default_eip1654_validator().map(|validator| &**validator),
    )
    .await
}

pub async fn try_extract_signer(headers: &HeaderMap, method: &str, path: &str) -> Option<Signer> {
    verify(headers, method, path).await.ok()
}

/// Mirrors @dcl/crypto-middleware >=5.1.0 (pulled in by events#1007's
/// decentraland-gatsby 8.4.8 bump): a signed-fetch request whose
/// `x-identity-metadata` JSON carries a `signer` or `intent` that is not already
/// its own `trim().to_lowercase()` is rejected with a 400 message prefixed
/// `Invalid chain metadata: `. Metadata without `signer`/`intent`, or non-JSON
/// metadata, is unaffected.
///
/// The gate must exist because the payload is lowercased before signing while the
/// header keeps its original casing: a mixed-case `Decentraland-Kernel-Scene`
/// signs byte-identically to the canonical spelling, so without this a scene could
/// present a scene-signed request as a directly user-signed action (e.g. silently
/// RSVPing a visiting player). Matches catalyrst-market's wording for cross-service
/// parity; kept crate-local like market's copy rather than shared through
/// catalyrst-crypto.
fn check_canonical_metadata(metadata: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return Ok(());
    };
    for key in ["signer", "intent"] {
        if let Some(raw) = value.get(key).and_then(serde_json::Value::as_str) {
            if raw != raw.trim().to_lowercase() {
                return Err(format!("Invalid chain metadata: {}", echo(metadata)));
            }
        }
    }
    Ok(())
}

const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// Mirrors decentraland-gatsby's default `verifySigner` metadataValidator, wired
/// onto every `auth()`/`auth({optional:true})` route in upstream events. It
/// throws `RequestError('Invalid signer', 400)` when the `x-identity-metadata`
/// `signer` is the kernel-scene signer -- a scene-originated request presenting
/// itself as a directly user-signed action. The canonical gate above already
/// forces a string `signer` to `trim().to_lowercase()`, so an exact match here
/// catches the mixed-case escape (it 400s at the canonical gate first); a
/// non-string `signer`, which both gates skip, is refused by
/// [`scene_signer_gate`] inside [`verify_with`].
fn check_metadata_signer(metadata: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return Ok(());
    };
    if value.get("signer").and_then(serde_json::Value::as_str) == Some(SCENE_SIGNER) {
        return Err("Invalid signer".to_string());
    }
    Ok(())
}

/// The single choke point every mutating events route funnels through. The
/// canonical-metadata gate runs first (400 on a non-canonical `signer`/`intent`),
/// then [`verify`] checks the auth chain (401 on any failure, key spellings
/// included), so every current and future authenticated handler inherits both.
/// Read-only GET routes stay unauthenticated and use [`try_extract_signer`]
/// instead.
pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, ApiError> {
    let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    check_canonical_metadata(metadata).map_err(ApiError::bad_request)?;
    check_metadata_signer(metadata).map_err(ApiError::bad_request)?;
    verify(headers, method, path)
        .await
        .map_err(|_| ApiError::unauthorized("Unauthorized"))
}

/// Optional-auth counterpart of [`require_signer`], mirroring gatsby's
/// `auth({optional:true})`: a request with no auth-chain headers resolves to
/// `None`, but one that DOES present headers still runs the canonical and
/// verifySigner metadata gates, so a scene-signed or non-canonical read is
/// rejected 400 rather than silently served as anonymous. A merely unverifiable
/// signature still degrades to anonymous, matching crypto-middleware's optional
/// verify.
pub async fn optional_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Option<Signer>, ApiError> {
    let first_link = format!("{AUTH_CHAIN_HEADER_PREFIX}0");
    if signed_fetch::header_str(headers, &first_link).is_some() {
        let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
        check_canonical_metadata(metadata).map_err(ApiError::bad_request)?;
        check_metadata_signer(metadata).map_err(ApiError::bad_request)?;
    }
    Ok(try_extract_signer(headers, method, path).await)
}

#[cfg(test)]
mod canonical_metadata_tests {
    use super::*;

    fn status(e: ApiError) -> u16 {
        match e {
            ApiError::Common(catalyrst_types::ApiError::Http { status, .. }) => status,
            _ => 0,
        }
    }

    #[test]
    fn rejects_non_canonical_signer_and_intent() {
        for meta in [
            // The exploit this closes: a mixed-case kernel-scene signer.
            r#"{"origin":"https://play.decentraland.org","signer":"Decentraland-Kernel-Scene"}"#,
            r#"{"signer":" dcl:marketplace"}"#,
            r#"{"intent":"Dcl:Intent"}"#,
            r#"{"intent":"dcl:intent "}"#,
        ] {
            let err = check_canonical_metadata(meta).expect_err(meta);
            assert!(
                err.starts_with("Invalid chain metadata: "),
                "message must match upstream prefix, got: {err}"
            );
        }
    }

    #[test]
    fn accepts_canonical_and_absent_metadata() {
        assert!(check_canonical_metadata(r#"{"signer":"decentraland-kernel-scene"}"#).is_ok());
        assert!(check_canonical_metadata(r#"{"intent":"dcl:intent"}"#).is_ok());
        assert!(check_canonical_metadata("{}").is_ok());
        assert!(check_canonical_metadata("not json").is_ok());
    }

    /// The gate fires before signature verification, so a valid-looking request
    /// carrying non-canonical metadata is a 400, not a 401.
    #[tokio::test]
    async fn require_signer_rejects_non_canonical_metadata_with_400() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            r#"{"signer":"Decentraland-Kernel-Scene"}"#.parse().unwrap(),
        );
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("non-canonical metadata must be rejected");
        assert_eq!(status(err), 400);
    }

    #[tokio::test]
    async fn require_signer_missing_auth_is_401_not_400() {
        let headers = HeaderMap::new();
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("missing auth must be rejected");
        assert_eq!(status(err), 401);
    }

    #[test]
    fn rejects_canonical_scene_signer() {
        let err = check_metadata_signer(r#"{"signer":"decentraland-kernel-scene"}"#)
            .expect_err("kernel-scene signer must be rejected");
        assert_eq!(err, "Invalid signer");
        assert!(check_metadata_signer(r#"{"signer":"0xabc"}"#).is_ok());
        assert!(check_metadata_signer("{}").is_ok());
        assert!(check_metadata_signer("not json").is_ok());
    }

    /// The canonical spelling clears the casing gate but is still a scene
    /// impersonation -- require_signer 400s it with gatsby's "Invalid signer".
    #[tokio::test]
    async fn require_signer_rejects_canonical_scene_signer_with_400() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            r#"{"signer":"decentraland-kernel-scene"}"#.parse().unwrap(),
        );
        let err = require_signer(&headers, "post", "/api/events")
            .await
            .expect_err("canonical scene signer must be rejected");
        assert_eq!(status(err), 400);
    }
}

#[cfg(test)]
mod signed_fetch_payload_tests {
    use super::*;
    use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
    use catalyrst_crypto::Wallet;
    use serde_json::json;

    const USER_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";
    const METHOD: &str = "post";
    const PATH: &str = "/api/events/e1/attendees";

    fn status(e: ApiError) -> u16 {
        match e {
            ApiError::Common(catalyrst_types::ApiError::Http { status, .. }) => status,
            _ => 0,
        }
    }

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
        headers.insert(AUTH_TIMESTAMP_HEADER, ts.parse().unwrap());
        headers.insert(AUTH_METADATA_HEADER, metadata.parse().unwrap());
        headers
    }

    fn now_ms() -> String {
        chrono::Utc::now().timestamp_millis().to_string()
    }

    /// Every in-tree signer still mints the folded payload, and the explorer
    /// handshake metadata carries a cased `origin`, so the two payload shapes
    /// differ here and the fallback is what keeps the request served.
    #[tokio::test]
    async fn legacy_signed_request_with_canonical_keys_is_still_served() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let metadata = json!({
            "signer": "dcl:explorer",
            "intent": "dcl:explorer:comms-handshake",
            "origin": "https://Play.Decentraland.org",
        })
        .to_string();
        let payload = build_legacy_payload(METHOD, PATH, &ts, &metadata);
        let signer = require_signer(
            &headers_for(&wallet, &payload, &ts, &metadata),
            METHOD,
            PATH,
        )
        .await
        .expect("legacy-signed request must still verify");
        assert_eq!(signer.as_str(), wallet.address().to_lowercase());
    }

    #[tokio::test]
    async fn v6_signed_request_with_cased_metadata_is_served() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let metadata = json!({ "origin": "https://Play.Decentraland.org" }).to_string();
        let payload = build_payload_v6(METHOD, PATH, &ts, &metadata);
        let signer = require_signer(
            &headers_for(&wallet, &payload, &ts, &metadata),
            METHOD,
            PATH,
        )
        .await
        .expect("v6-signed request must verify");
        assert_eq!(signer.as_str(), wallet.address().to_lowercase());
    }

    /// The bypass this migration closes. One folded payload covers both
    /// spellings, so the chain below is genuinely valid for the delivered
    /// bytes; with the key unguarded, both gates read `signer` as absent and a
    /// scene-signed RSVP is served as the visiting player's own.
    #[tokio::test]
    async fn respelled_signer_key_is_refused_not_served_as_a_user() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let signed = json!({ "signer": "decentraland-kernel-scene" }).to_string();
        let delivered = json!({ "Signer": "decentraland-kernel-scene" }).to_string();
        let payload = build_legacy_payload(METHOD, PATH, &ts, &signed);
        assert_eq!(payload, build_legacy_payload(METHOD, PATH, &ts, &delivered));

        let err = require_signer(
            &headers_for(&wallet, &payload, &ts, &delivered),
            METHOD,
            PATH,
        )
        .await
        .expect_err("a re-spelled signer key must not be served");
        assert_eq!(status(err), 401);

        let err = require_signer(&headers_for(&wallet, &payload, &ts, &signed), METHOD, PATH)
            .await
            .expect_err("the same chain spelled canonically is a scene request");
        assert_eq!(status(err), 400);
    }

    /// Same shape one field over: `intent` is guarded because the canonical
    /// gate decides on it, so re-spelling its key must not silence that gate.
    #[tokio::test]
    async fn respelled_intent_key_is_refused() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let delivered = json!({ "Intent": "Dcl:Intent" }).to_string();
        let payload = build_legacy_payload(METHOD, PATH, &ts, &delivered);
        let err = require_signer(
            &headers_for(&wallet, &payload, &ts, &delivered),
            METHOD,
            PATH,
        )
        .await
        .expect_err("a re-spelled intent key must not be served");
        assert_eq!(status(err), 401);
    }

    /// Optional routes degrade to anonymous rather than to the signing user:
    /// serving the re-spelled request as `Some(address)` is the same bypass.
    #[tokio::test]
    async fn respelled_signer_key_reads_as_anonymous_on_optional_routes() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let delivered = json!({ "Signer": "decentraland-kernel-scene" }).to_string();
        let payload = build_legacy_payload("get", PATH, &ts, &delivered);
        let resolved = optional_signer(
            &headers_for(&wallet, &payload, &ts, &delivered),
            "get",
            PATH,
        )
        .await
        .expect("an optional route must not 400 on a re-spelled key");
        assert_eq!(resolved, None);
    }

    /// Both crate-local gates read the `signer` through `as_str()`, so a scene
    /// signer wrapped in an array reads as absent to them; upstream's gate
    /// refuses any `signer` that is not a canonical string.
    #[tokio::test]
    async fn non_string_scene_signer_is_refused() {
        let wallet = Wallet::from_hex(USER_KEY).unwrap();
        let ts = now_ms();
        let metadata = json!({ "signer": ["decentraland-kernel-scene"] }).to_string();
        let payload = build_legacy_payload(METHOD, PATH, &ts, &metadata);
        let err = require_signer(
            &headers_for(&wallet, &payload, &ts, &metadata),
            METHOD,
            PATH,
        )
        .await
        .expect_err("a non-string scene signer must not be served");
        assert_eq!(status(err), 401);
    }
}

/// Pins the *number* of signature validations, not just the outcome. An
/// EIP-1654 chain is the shape where a second attempt is not free: each
/// validation is an `isValidSignature` call on the signer's smart-contract
/// wallet, so a payload shape tried and discarded is a catalyst round-trip
/// spent on the authenticating path of every request.
#[cfg(test)]
mod signature_attempt_cost_tests {
    use super::*;
    use async_trait::async_trait;
    use catalyrst_crypto::signed_fetch::{build_legacy_payload, build_payload_v6};
    use catalyrst_crypto::AuthError;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const CONTRACT: &str = "0x1234567890123456789012345678901234567890";
    const SIGNATURE: &str = "0xab";
    const METHOD: &str = "post";
    const PATH: &str = "/api/events/e1/attendees";

    /// Accepts one message and counts every call; a call is one catalyst
    /// round-trip. `verify_eip1654` asks a second time with the eip-191
    /// prefixed hash only after the raw keccak hash is refused, so an attempt
    /// the wallet accepts costs 1 and one it refuses costs 2. The wallet is
    /// asked about the chain link's own payload, so an attempt it accepts can
    /// still fail -- on the payload the verified chain is then compared to.
    struct CountingValidator {
        accepted: Vec<[u8; 32]>,
        calls: AtomicUsize,
    }

    impl CountingValidator {
        fn accepting(message: &str) -> Self {
            Self {
                accepted: vec![
                    alloy_primitives::keccak256(message.as_bytes()).0,
                    alloy_primitives::eip191_hash_message(message.as_bytes()).0,
                ],
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Eip1654Validator for CountingValidator {
        async fn validate_signature(
            &self,
            _contract_address: &str,
            hash: &[u8],
            _signature: &[u8],
        ) -> Result<bool, AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.accepted.iter().any(|known| known.as_slice() == hash))
        }
    }

    fn headers_for(payload: &str, timestamp: &str, metadata: &str) -> HeaderMap {
        let link0 = json!({ "type": "SIGNER", "payload": CONTRACT, "signature": "" });
        let link1 = json!({
            "type": "ECDSA_EIP_1654_SIGNED_ENTITY",
            "payload": payload,
            "signature": SIGNATURE,
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-identity-auth-chain-0",
            link0.to_string().parse().unwrap(),
        );
        headers.insert(
            "x-identity-auth-chain-1",
            link1.to_string().parse().unwrap(),
        );
        headers.insert(AUTH_TIMESTAMP_HEADER, timestamp.parse().unwrap());
        headers.insert(AUTH_METADATA_HEADER, metadata.parse().unwrap());
        headers
    }

    fn now_ms() -> String {
        chrono::Utc::now().timestamp_millis().to_string()
    }

    /// The explorer's handshake metadata: camelCase by construction, so the two
    /// payload shapes differ and an ordering mistake is paid on every request.
    fn explorer_metadata() -> String {
        json!({
            "intent": "dcl:explorer:comms-handshake",
            "signer": "dcl:explorer",
            "isGuest": false,
        })
        .to_string()
    }

    #[tokio::test]
    async fn a_legacy_signed_request_costs_one_validation() {
        let ts = now_ms();
        let metadata = explorer_metadata();
        let legacy = build_legacy_payload(METHOD, PATH, &ts, &metadata);
        assert_ne!(legacy, build_payload_v6(METHOD, PATH, &ts, &metadata));

        let validator = CountingValidator::accepting(&legacy);
        let signer = verify_with(
            &headers_for(&legacy, &ts, &metadata),
            METHOD,
            PATH,
            Some(&validator),
        )
        .await
        .expect("a legacy-signed request must verify");

        assert_eq!(signer.as_str(), CONTRACT);
        assert_eq!(
            validator.calls(),
            1,
            "the shape our own fleet signs must not pay for a discarded attempt"
        );
    }

    /// The negative control for the count above: with `{}` there is one payload,
    /// not two, so even a refused request must not be validated twice over.
    #[tokio::test]
    async fn identical_payloads_are_never_attempted_twice() {
        let ts = now_ms();
        let legacy = build_legacy_payload(METHOD, PATH, &ts, "{}");
        assert_eq!(legacy, build_payload_v6(METHOD, PATH, &ts, "{}"));

        let validator = CountingValidator::accepting("not the delivered payload");
        let err = verify_with(
            &headers_for(&legacy, &ts, "{}"),
            METHOD,
            PATH,
            Some(&validator),
        )
        .await
        .expect_err("a signature the wallet refuses must not verify");

        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
        assert_eq!(validator.calls(), 2, "one attempt is two hash forms");
    }

    /// The fallback still exists: a 6.x-signed request whose metadata is cased
    /// verifies, at the cost of the legacy attempt that preceded it.
    #[tokio::test]
    async fn a_v6_signed_request_still_verifies_through_the_fallback() {
        let ts = now_ms();
        let metadata = explorer_metadata();
        let v6 = build_payload_v6(METHOD, PATH, &ts, &metadata);

        let validator = CountingValidator::accepting(&v6);
        let signer = verify_with(
            &headers_for(&v6, &ts, &metadata),
            METHOD,
            PATH,
            Some(&validator),
        )
        .await
        .expect("a 6.x-signed request must verify");

        assert_eq!(signer.as_str(), CONTRACT);
        assert_eq!(
            validator.calls(),
            2,
            "the discarded legacy attempt, then the 6.x one that settles it"
        );
    }

    /// The security property the ordering must not cost. The wallet below
    /// accepts the delivered chain, so had the legacy attempt been made the
    /// re-spelled `signer` key would have been served as the contract's own
    /// request; the canonical-key guard runs first and removes that attempt,
    /// which is why the only verdict left to refuse the request is the 6.x
    /// signature's.
    #[tokio::test]
    async fn a_respelled_key_never_reaches_the_legacy_attempt() {
        let ts = now_ms();
        let delivered = json!({ "Signer": "decentraland-kernel-scene" }).to_string();
        let legacy = build_legacy_payload(METHOD, PATH, &ts, &delivered);

        let validator = CountingValidator::accepting(&legacy);
        let err = verify_with(
            &headers_for(&legacy, &ts, &delivered),
            METHOD,
            PATH,
            Some(&validator),
        )
        .await
        .expect_err("a re-spelled signer key must not be served");

        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
        assert_eq!(
            validator.calls(),
            1,
            "the guard answers first, so only the 6.x attempt is ever made"
        );
    }

    /// A non-signature failure is deterministic in the payload shape, so it
    /// answers on the first attempt rather than paying a second round-trip.
    #[tokio::test]
    async fn an_expired_request_pays_no_validation_at_all() {
        let ts = (chrono::Utc::now().timestamp_millis() - (FIVE_MINUTES + 60) * 1000).to_string();
        let metadata = explorer_metadata();
        let legacy = build_legacy_payload(METHOD, PATH, &ts, &metadata);

        let validator = CountingValidator::accepting(&legacy);
        let err = verify_with(
            &headers_for(&legacy, &ts, &metadata),
            METHOD,
            PATH,
            Some(&validator),
        )
        .await
        .expect_err("a stale request must not verify");

        assert!(matches!(err, AuthChainError::Expired { .. }), "{err:?}");
        assert_eq!(validator.calls(), 0);
    }
}
