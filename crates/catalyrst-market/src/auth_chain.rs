use std::sync::OnceLock;

use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch;
use catalyrst_crypto::{
    reject_if_signer, require_canonical_field, require_signer as signer_gate, RequiredFieldGate,
    Signer, SignerGate,
};
use catalyrst_types::AuthLinkType;

use crate::http::response::ApiError;

pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthChainError, AuthLink, AUTH_CHAIN_HEADER_PREFIX,
    AUTH_METADATA_HEADER, AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES: i64 = 5 * 60;

/// The `signer` an explorer sets on an auth chain it signed on a scene's behalf.
pub const SCENE_SIGNER: &str = "decentraland-kernel-scene";

/// Upstream's `validateNotKernelSceneSigner` (marketplace-server
/// src/controllers/utils.ts:9-16), the `metadataValidator` of /v1/catalog,
/// /v2/catalog, /v1/wert/sign, /v1/transak/orders, /v1/nfts, /v1/items and every
/// favorites/picks/lists route (routes.ts:73,82,91,100,124,161,177 and
/// favorites/routes.ts:30-158).
///
/// `rejectIfSigner` refuses a `signer` that is not already canonical instead of
/// folding it before comparing: the pre-6.0.0 payload lowercased the metadata
/// before signing, so `{"Signer":"Decentraland-Kernel-Scene"}` carries a signature
/// byte-identical to the canonical spelling's while a folding comparison decides on
/// a value the handler never sees. Nothing is rewritten.
fn scene_signer_gate() -> &'static SignerGate {
    static GATE: OnceLock<SignerGate> = OnceLock::new();
    GATE.get_or_init(|| reject_if_signer(&[SCENE_SIGNER]).expect("SCENE_SIGNER is canonical"))
}

/// Upstream's `verifyMetadata` refuses an unparseable or non-object
/// `x-identity-metadata` outright, before any validator runs, and reads an explicit
/// JSON `null` as an empty object. Coercing it instead let every gate below pass
/// vacuously over a delivery upstream drops.
fn metadata_object(raw: &str) -> Result<serde_json::Value, ApiError> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Null) => Ok(serde_json::Value::Object(serde_json::Map::new())),
        Ok(value @ serde_json::Value::Object(_)) => Ok(value),
        _ => {
            let echo: String = raw.chars().take(64).collect();
            Err(ApiError::bad_request(format!(
                "Invalid chain metadata: {echo}"
            )))
        }
    }
}

/// Reads `x-identity-metadata`, defaulting to `{}` like the signature path does,
/// and runs [`scene_signer_gate`] over it. Upstream answers 400 "Invalid signer".
pub fn require_not_scene_signer(headers: &HeaderMap) -> Result<(), ApiError> {
    let raw = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata = metadata_object(raw)?;
    if !scene_signer_gate().permits(&metadata) {
        return Err(ApiError::bad_request("Invalid signer"));
    }
    Ok(())
}

/// Upstream installs this on the routes a marketplace or builder client is the only
/// legitimate caller of (routes.ts:122,165).
pub const MARKETPLACE_AUTH_SIGNERS: &[&str] = &["dcl:marketplace", "dcl:builder"];

/// The intent upstream demands of POST /v1/trades (routes.ts:122).
pub const CREATE_TRADE_INTENT: &str = "dcl:create-trade";

/// The intent upstream demands of POST /v1/coupons (routes.ts:127).
pub const CREATE_COUPON_INTENT: &str = "dcl:create-coupon";

/// Upstream `validateAuthMetadata(signers, intent)` (marketplace-server
/// src/controllers/utils.ts), the route-level policy behind POST /v1/trades and
/// GET /v1/activity.
///
/// Both comparisons are exact against canonical declarations, with nothing folded first:
/// @dcl/crypto-middleware 6.x hands the validator the metadata exactly as signed, and the
/// legacy payload lowercases it before signing, so a re-spelled `Dcl:Marketplace` or
/// `Decentraland-Kernel-Scene` carries a byte-identical signature. Folding before comparing
/// would authorize a request as something it is not -- the whole point of upstream #393.
///
/// The read, the form check and the comparison all happen inside the shared gate, so
/// no plain field read here can reintroduce a spelling it refused. Both helpers
/// refuse a non-canonical declaration at construction, as upstream does when the
/// route is defined.
pub fn require_auth_metadata(
    headers: &HeaderMap,
    allowed_signers: &[&str],
    intent: Option<&str>,
) -> Result<(), ApiError> {
    let raw = signed_fetch::header_str(headers, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata = metadata_object(raw)?;

    let signer = signer_gate(allowed_signers).expect("route signers are canonical");
    if !signer.permits(&metadata) {
        return Err(ApiError::bad_request("Invalid auth signer"));
    }

    if let Some(expected) = intent {
        let gate: RequiredFieldGate =
            require_canonical_field("intent", &[expected]).expect("route intent is canonical");
        if !gate.permits(&metadata) {
            return Err(ApiError::bad_request(
                "Invalid auth intent to perform this operation",
            ));
        }
    }

    Ok(())
}

/// Matches upstream marketplace-server wording; everything not explicitly special-cased is
/// "Invalid Auth Chain".
pub trait AuthChainErrorExt {
    fn message(&self) -> String;
}

impl AuthChainErrorExt for AuthChainError {
    fn message(&self) -> String {
        match self {
            AuthChainError::AddressMismatch { .. } => "Forbidden: address mismatch".to_string(),
            AuthChainError::Expired { .. } => "Expired signature".to_string(),
            AuthChainError::EipNotImplemented => "EIP-1654 not supported on this route".to_string(),

            _ => "Invalid Auth Chain".to_string(),
        }
    }
}

/// market never surfaces ForbiddenSigner: folding it into InvalidSignature preserves the
/// pre-consolidation route behavior (401, not a 400 fallthrough).
fn normalize(e: AuthChainError) -> AuthChainError {
    match e {
        AuthChainError::ForbiddenSigner => AuthChainError::InvalidSignature(e.to_string()),
        other => other,
    }
}

fn reject_eip_links(chain: &AuthChain) -> Result<(), AuthChainError> {
    for link in &chain.links {
        if matches!(
            link.kind,
            AuthLinkType::EcdsaEip1654Ephemeral | AuthLinkType::EcdsaEip1654SignedEntity
        ) {
            return Err(AuthChainError::EipNotImplemented);
        }
    }
    Ok(())
}

pub fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    let chain = signed_fetch::extract_auth_chain(headers).map_err(normalize)?;
    reject_eip_links(&chain)?;
    Ok(chain)
}

pub async fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<Signer, AuthChainError> {
    signed_fetch::validate_signature(chain, payload, timestamp, expiration_secs, now)
        .await
        .map_err(normalize)
}

/// The 6.x payload first, the legacy one only on a signature mismatch, so a legacy-signed
/// request answers exactly as it did before.
pub async fn validate_signature_either_payload<'a>(
    chain: &AuthChain,
    method: &str,
    path: impl Into<signed_fetch::SignedFetchPath<'a>>,
    timestamp: &str,
    metadata: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<Signer, AuthChainError> {
    signed_fetch::validate_signature_either_payload(
        chain,
        method,
        path,
        timestamp,
        metadata,
        expiration_secs,
        now,
    )
    .await
    .map_err(normalize)
}

pub async fn verify_with_address(
    chain: &AuthChain,
    payload: &str,
    timestamp: &str,
    expiration_secs: i64,
    now: i64,
    expected_address: &str,
) -> Result<Signer, AuthChainError> {
    let recovered = validate_signature(chain, payload, timestamp, expiration_secs, now).await?;
    if recovered.as_str() != expected_address.to_lowercase() {
        return Err(AuthChainError::AddressMismatch {
            expected: expected_address.to_lowercase(),
            recovered: recovered.as_str().to_string(),
        });
    }
    Ok(recovered)
}

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    let path = signed_fetch::signed_fetch_path(headers, path);
    let chain = extract_auth_chain(headers)?;
    let ts = signed_fetch::header_str(headers, AUTH_TIMESTAMP_HEADER)
        .ok_or(AuthChainError::MissingTimestamp)?
        .to_string();
    let metadata = signed_fetch::header_str(headers, AUTH_METADATA_HEADER)
        .unwrap_or("{}")
        .to_string();
    let now = chrono::Utc::now().timestamp();
    validate_signature_either_payload(&chain, method, path, &ts, &metadata, FIVE_MINUTES, now).await
}

fn auth_chain_error_to_api(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::EipNotImplemented => {
            ApiError::Http(catalyrst_types::HttpError::new(501, e.message()))
        }
        _ => ApiError::Http(catalyrst_types::HttpError::new(401, e.message())),
    }
}

/// Upstream's `wellKnownComponents({ optional: true })` swallows a verification
/// failure - the `metadataValidator` refusal included (core-libs
/// crypto-middleware src/index.ts:92-98) - and continues with no verification
/// rather than answering, so a scene-signed catalog or picks-list request reads as
/// a signed-out visitor instead of 400.
pub async fn optional_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Option<String>, ApiError> {
    let first_link = format!("{AUTH_CHAIN_HEADER_PREFIX}0");
    if !headers.contains_key(first_link.as_str()) {
        return Ok(None);
    }
    if require_not_scene_signer(headers).is_err() {
        return Ok(None);
    }
    require_signer(headers, method, path)
        .await
        .map(|s| Some(s.as_str().to_string()))
        .map_err(auth_chain_error_to_api)
}

#[cfg(test)]
mod scene_signer_gate_tests {
    use super::{require_not_scene_signer, AUTH_METADATA_HEADER};
    use axum::http::{HeaderMap, HeaderValue};

    fn headers(metadata: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        headers
    }

    fn refusal(metadata: &str) -> String {
        let err = require_not_scene_signer(&headers(metadata)).expect_err(metadata);
        format!("{err:?}")
    }

    /// The hole this gate closes: a scene runtime signs on the visiting player's
    /// behalf and every favorites, picks, lists and catalog route served it as that
    /// player, because the check it replaced only compared the value to its own
    /// `trim().to_lowercase()`.
    #[test]
    fn the_canonical_scene_signer_is_refused() {
        assert!(refusal(r#"{"signer":"decentraland-kernel-scene"}"#).contains("Invalid signer"));
        assert!(refusal(
            r#"{"origin":"https://play.decentraland.org","signer":"decentraland-kernel-scene"}"#
        )
        .contains("Invalid signer"));
    }

    /// The fold is why the gate refuses a non-canonical spelling rather than
    /// comparing it: under the legacy payload these carry a signature identical to
    /// the canonical spelling's.
    #[test]
    fn a_folded_or_re_cased_signer_is_refused() {
        for metadata in [
            r#"{"Signer":"decentraland-kernel-scene"}"#,
            r#"{"signer":"Decentraland-Kernel-Scene"}"#,
            r#"{"signer":"decentraland-kernel-scene","Signer":"x"}"#,
            r#"{"signer":" dcl:marketplace"}"#,
            r#"{"signer":"Dcl:Marketplace"}"#,
        ] {
            assert!(
                refusal(metadata).contains("Invalid signer"),
                "{metadata} must be refused"
            );
        }
    }

    #[test]
    fn an_ordinary_signer_and_absent_metadata_pass() {
        for metadata in [
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#,
            r#"{"intent":"dcl:marketplace:remove-pick"}"#,
            "{}",
            "null",
        ] {
            assert!(
                require_not_scene_signer(&headers(metadata)).is_ok(),
                "{metadata} must be served"
            );
        }
        assert!(require_not_scene_signer(&HeaderMap::new()).is_ok());
    }

    /// The optional routes (/v1/catalog, /v1/lists/:id/picks) reproduce upstream's
    /// `optional: true`: a refused request reads as a signed-out visitor, never as
    /// the visiting user.
    #[tokio::test]
    async fn an_optional_route_reads_a_refused_request_as_anonymous() {
        for metadata in [
            r#"{"signer":"decentraland-kernel-scene"}"#,
            r#"{"Signer":"decentraland-kernel-scene"}"#,
            "not json",
        ] {
            let mut h = headers(metadata);
            h.insert(
                axum::http::HeaderName::from_static("x-identity-auth-chain-0"),
                HeaderValue::from_static("{}"),
            );
            assert_eq!(
                super::optional_signer(&h, "get", "/v1/catalog").await.ok(),
                Some(None),
                "{metadata}"
            );
        }
    }

    /// Upstream's `verifyMetadata` 400s on these before any validator runs; ours
    /// used to return Ok and let every gate pass vacuously.
    #[test]
    fn unparseable_or_non_object_metadata_is_refused() {
        for metadata in ["not json", "[]", r#""a string""#, "7"] {
            assert!(
                refusal(metadata).contains("Invalid chain metadata: "),
                "{metadata} must be refused"
            );
        }
    }
}

#[cfg(test)]
mod auth_metadata_tests {
    use super::{
        require_auth_metadata, AUTH_METADATA_HEADER, CREATE_TRADE_INTENT, MARKETPLACE_AUTH_SIGNERS,
    };
    use axum::http::{HeaderMap, HeaderValue};

    fn headers(metadata: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTH_METADATA_HEADER,
            HeaderValue::from_str(metadata).unwrap(),
        );
        h
    }

    fn message(err: crate::http::response::ApiError) -> String {
        err.to_string()
    }

    #[test]
    fn accepts_every_allowed_signer_with_the_declared_intent() {
        for signer in MARKETPLACE_AUTH_SIGNERS {
            let metadata = format!(r#"{{"signer":"{signer}","intent":"{CREATE_TRADE_INTENT}"}}"#);
            assert!(require_auth_metadata(
                &headers(&metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT)
            )
            .is_ok());
        }
    }

    #[test]
    fn refuses_a_signer_outside_the_allow_list() {
        for metadata in [
            r#"{"signer":"decentraland-kernel-scene","intent":"dcl:create-trade"}"#,
            r#"{"signer":"Decentraland-Kernel-Scene","intent":"dcl:create-trade"}"#,
            r#"{"signer":"dcl:explorer","intent":"dcl:create-trade"}"#,
            r#"{"signer":"Dcl:Marketplace","intent":"dcl:create-trade"}"#,
            r#"{"signer":" dcl:marketplace","intent":"dcl:create-trade"}"#,
            r#"{"signer":42,"intent":"dcl:create-trade"}"#,
            r#"{"Signer":"dcl:marketplace","intent":"dcl:create-trade"}"#,
            r#"{"signer":"dcl:marketplace","Signer":"x","intent":"dcl:create-trade"}"#,
            r#"{"intent":"dcl:create-trade"}"#,
            "{}",
        ] {
            let err = require_auth_metadata(
                &headers(metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT),
            )
            .expect_err(metadata);
            assert_eq!(message(err), "Invalid auth signer", "{metadata}");
        }
    }

    #[test]
    fn refuses_a_wrong_absent_or_respelled_intent() {
        for metadata in [
            r#"{"signer":"dcl:marketplace"}"#,
            r#"{"signer":"dcl:marketplace","intent":"dcl:marketplace:add-pick"}"#,
            r#"{"signer":"dcl:marketplace","intent":"Dcl:Create-Trade"}"#,
            r#"{"signer":"dcl:marketplace","intent":"dcl:create-trade "}"#,
            r#"{"signer":"dcl:marketplace","intent":null}"#,
        ] {
            let err = require_auth_metadata(
                &headers(metadata),
                MARKETPLACE_AUTH_SIGNERS,
                Some(CREATE_TRADE_INTENT),
            )
            .expect_err(metadata);
            assert_eq!(
                message(err),
                "Invalid auth intent to perform this operation",
                "{metadata}"
            );
        }
    }

    #[test]
    fn leaves_intent_alone_when_the_route_declares_none() {
        for metadata in [
            r#"{"signer":"dcl:marketplace"}"#,
            r#"{"signer":"dcl:builder","intent":"dcl:marketplace:add-pick"}"#,
        ] {
            assert!(
                require_auth_metadata(&headers(metadata), MARKETPLACE_AUTH_SIGNERS, None).is_ok(),
                "{metadata}"
            );
        }
    }

    #[test]
    fn a_missing_metadata_header_carries_no_signer() {
        let err = require_auth_metadata(&HeaderMap::new(), MARKETPLACE_AUTH_SIGNERS, None)
            .expect_err("no metadata header means no signer");
        assert_eq!(message(err), "Invalid auth signer");
    }

    /// Upstream refuses an unparseable or non-object header in `verifyMetadata`,
    /// before the route validator is reached, so it never reads as "no signer".
    #[test]
    fn unparseable_metadata_is_refused_before_the_signer_is_read() {
        for metadata in ["not json", "[]", "7"] {
            let err = require_auth_metadata(&headers(metadata), MARKETPLACE_AUTH_SIGNERS, None)
                .expect_err(metadata);
            assert!(
                message(err).starts_with("Invalid chain metadata: "),
                "{metadata}"
            );
        }
    }
}
