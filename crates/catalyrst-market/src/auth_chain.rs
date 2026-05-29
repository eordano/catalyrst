//! Auth-chain extraction + signature verification for marketplace HTTP routes.
//!
//! Mirrors `@dcl/crypto-middleware`'s `extractAuthChain` + `validateSignature`
//! semantics — but in Rust, against the canonical
//! `<method>:<path>:<timestamp>:<metadata>` (all lowercase) personal_sign
//! payload.
//!
//! Why this lives in `catalyrst-market` and not `catalyrst-crypto`:
//!   - `catalyrst-crypto` already exposes the EIP-191 / personal_sign
//!     primitives (`recover_address`, `verify_auth_chain`) and the canonical
//!     `AuthChain`/`AuthLink`/`AuthLinkType` types.
//!   - This module is a thin HTTP-layer adapter: it pulls the `x-identity-*`
//!     headers off `axum::http::HeaderMap`, runs the crypto verifier from
//!     `catalyrst-crypto`, and returns an error envelope that the marketplace
//!     handlers can `?`-bubble up.
//!
//! The shape (struct names, link variants, function signatures) follows the
//! spec — `AuthChain { links, signer }`, three-variant `AuthLinkType`,
//! `extract_auth_chain(&HeaderMap)` + `validate_signature(&chain, payload,
//! expiration, now)` — even though the underlying primitives live in
//! `catalyrst-crypto`. We re-export the canonical `AuthLinkType` from
//! `catalyrst-types` for the three personal_sign variants the spec calls out;
//! EIP-1654 variants are accepted at the parse step but rejected (with a clear
//! error) at validation time.

use axum::http::HeaderMap;
use catalyrst_crypto::verify::verify_auth_chain;
use catalyrst_crypto::AuthError;
use catalyrst_types::{AuthLink as CryptoAuthLink, AuthLinkType as CryptoAuthLinkType, EthAddress};
use thiserror::Error;

/// Header prefix for the JSON-encoded auth-chain links.
/// See `@dcl/crypto`'s `AUTH_CHAIN_HEADER_PREFIX`.
pub const AUTH_CHAIN_HEADER_PREFIX: &str = "x-identity-auth-chain-";
/// Header carrying the unix-millis timestamp the client signed.
pub const AUTH_TIMESTAMP_HEADER: &str = "x-identity-timestamp";
/// Header carrying the JSON-encoded metadata object the client signed.
pub const AUTH_METADATA_HEADER: &str = "x-identity-metadata";

/// Max links in a single auth chain (matches `@dcl/crypto-middleware`).
pub const MAX_AUTH_CHAIN_LINKS: usize = 10;

/// 5-minute signature expiration window (seconds), per spec.
///
/// NOTE: the upstream marketplace-server uses `5 * 60 * 1000` (milliseconds)
/// because it works in JS `Date.now()` units. We work in
/// `chrono::Utc::now().timestamp()` (seconds), so the equivalent window is
/// `5 * 60`. The semantic — "reject signatures older than 5 minutes" — is
/// identical.
pub const FIVE_MINUTES: i64 = 5 * 60;

/// Personal_sign auth-chain link kind, restricted to the three variants the
/// spec enumerates. EIP-1654 contract-wallet variants are NOT modelled here;
/// they get rejected at verification time with `EipNotImplemented`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthLinkType {
    Signer,
    EcdsaPersonalEphemeral,
    EcdsaPersonalSignedEntity,
}

#[derive(Debug, Clone)]
pub struct AuthLink {
    pub kind: AuthLinkType,
    pub payload: String,
    pub signature: String,
}

/// Parsed + structurally validated auth chain.
///
/// `signer` is the **claimed** root address (the `SIGNER` link's payload),
/// NOT yet cryptographically verified. After `validate_signature` succeeds,
/// the returned `EthAddress` IS the verified signer (lowercased).
#[derive(Debug, Clone)]
pub struct AuthChain {
    pub links: Vec<AuthLink>,
    pub signer: EthAddress,
}

/// Errors that map onto specific HTTP responses in marketplace handlers.
///
/// Variants exist so handlers can pattern-match on intent (`AddressMismatch`
/// → 400 Forbidden envelope; `Expired` → 401; etc.) rather than parsing
/// stringly-typed messages.
#[derive(Debug, Error)]
pub enum AuthChainError {
    /// Shape-level failure: malformed JSON, missing required field, < 2 links,
    /// > MAX_AUTH_CHAIN_LINKS, non-string header value.
    #[error("Invalid Auth Chain")]
    MalformedChain { detail: String },

    /// Only one link supplied (the canonical "personal_sign" chain has ≥ 2:
    /// SIGNER + ECDSA_PERSONAL_*).
    #[error("Invalid Auth Chain")]
    InsufficientLinks,

    /// `x-identity-timestamp` missing or unparseable.
    #[error("Invalid Auth Chain")]
    MissingTimestamp,

    /// Signature is more than `expiration_secs` from `now`.
    #[error("Expired signature")]
    Expired {
        signed_at: i64,
        now: i64,
        window_secs: i64,
    },

    /// `secp256k1` recovery failed or the recovered address doesn't match the
    /// previous link's authority. (Tampered signature, mangled hex, etc.)
    #[error("Invalid signature")]
    InvalidSignature(String),

    /// Recovered signer ≠ `?address=` query string.
    #[error("Forbidden: address mismatch")]
    AddressMismatch { expected: String, recovered: String },

    /// EIP-1654 contract-wallet variants are accepted at parse time but
    /// require a Catalyst round-trip to validate. Marketplace handlers don't
    /// support that path today; surface a distinct error so callers know to
    /// gate the route on personal_sign only.
    #[error("EIP-1654 not implemented")]
    EipNotImplemented,
}

impl AuthChainError {
    /// Short label suitable for the `{ok:false, message:...}` envelope.
    ///
    /// Per spec: must stay `"Invalid Auth Chain"` for shape parity, EXCEPT
    /// the address-mismatch and expiration variants which have their own
    /// well-known strings. No echoing of serde parser-error tails.
    pub fn message(&self) -> String {
        match self {
            AuthChainError::AddressMismatch { .. } => "Forbidden: address mismatch".to_string(),
            AuthChainError::Expired { .. } => "Expired signature".to_string(),
            AuthChainError::EipNotImplemented => {
                "EIP-1654 not supported on this route".to_string()
            }
            // Every shape/sig-verify failure collapses to the canonical
            // bytes prod emits for `optional: false` routes. Anything richer
            // than this leaks parser internals to attackers.
            _ => "Invalid Auth Chain".to_string(),
        }
    }
}

/// Build the canonical personal_sign payload string for a given request.
///
/// Format: `"<method-lowercase>:<path-lowercase>:<timestamp>:<metadata>"`,
/// then lowercased end-to-end (matching `createPayload` in
/// `@dcl/crypto-middleware`).
pub fn build_payload(method: &str, path: &str, timestamp: &str, metadata: &str) -> String {
    format!("{}:{}:{}:{}", method, path, timestamp, metadata).to_lowercase()
}

/// Look up a header by lowercase name; return None if absent or non-ASCII.
fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Map an internal `AuthLinkType` from `catalyrst-types` onto our restricted
/// three-variant enum. Returns None for EIP-1654 variants — callers treat
/// that as "accepted shape, rejected at verification".
fn project_link_kind(kt: CryptoAuthLinkType) -> Option<AuthLinkType> {
    match kt {
        CryptoAuthLinkType::SIGNER => Some(AuthLinkType::Signer),
        CryptoAuthLinkType::EcdsaEphemeral => Some(AuthLinkType::EcdsaPersonalEphemeral),
        CryptoAuthLinkType::EcdsaSignedEntity => Some(AuthLinkType::EcdsaPersonalSignedEntity),
        // EIP-1654 variants parse-clean but are rejected at validate time.
        CryptoAuthLinkType::EcdsaEip1654Ephemeral
        | CryptoAuthLinkType::EcdsaEip1654SignedEntity => None,
    }
}

/// Parse `x-identity-auth-chain-N` headers (N = 0..MAX_AUTH_CHAIN_LINKS-1).
///
/// Each header value MUST be a JSON object `{type, payload, signature}` (all
/// strings; `signature` may be `""` for SIGNER links). Stops at the first
/// missing index. Rejects (and returns `MalformedChain`) on:
///   - non-UTF-8 / non-ASCII header byte;
///   - JSON parse failure;
///   - missing or non-string `type`/`payload`/`signature`;
///   - presence of `x-identity-auth-chain-{MAX_AUTH_CHAIN_LINKS}` (overflow);
///   - any link beyond the first whose `type` is `SIGNER` (only the first
///     link may be a SIGNER);
///   - first link not being `SIGNER`.
///
/// Rejects (and returns `InsufficientLinks`) on chain length < 2.
pub fn extract_auth_chain(headers: &HeaderMap) -> Result<AuthChain, AuthChainError> {
    let mut links = Vec::new();

    for i in 0..MAX_AUTH_CHAIN_LINKS {
        let name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, i);
        let Some(raw) = header_str(headers, &name) else {
            break;
        };

        // Parse the full link shape via the canonical serde repr in
        // catalyrst-types. This catches malformed `type` values and missing
        // `payload`/`signature` keys in one step.
        let link: CryptoAuthLink = serde_json::from_str(raw).map_err(|e| {
            // Don't echo serde's parser error tail to clients (security:
            // limits the size of attacker-controlled bytes in error logs and
            // response bodies). Keep a truncated tail for internal logs only.
            let mut detail = e.to_string();
            if detail.len() > 64 {
                detail.truncate(64);
            }
            AuthChainError::MalformedChain { detail }
        })?;

        // SIGNER links don't carry a signature in the wire format (it's
        // serialized as `Option<String>`). All other variants require a
        // non-empty signature. The structural check here matches the prod
        // middleware's `isValidAuthLink` (all three fields must be strings).
        match link.link_type {
            CryptoAuthLinkType::SIGNER => {
                if i != 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("SIGNER link at non-zero index {}", i),
                    });
                }
            }
            _ => {
                if i == 0 {
                    return Err(AuthChainError::MalformedChain {
                        detail: "first link must be SIGNER".to_string(),
                    });
                }
                if link.signature.as_deref().unwrap_or("").is_empty() {
                    return Err(AuthChainError::MalformedChain {
                        detail: format!("missing signature on link {}", i),
                    });
                }
            }
        }

        let kind = project_link_kind(link.link_type).ok_or(AuthChainError::EipNotImplemented)?;

        links.push(AuthLink {
            kind,
            payload: link.payload,
            signature: link.signature.unwrap_or_default(),
        });
    }

    // Overflow check: presence of header at index MAX is a hard reject (the
    // chain length was capped above so we'd otherwise silently truncate).
    let overflow_name = format!("{}{}", AUTH_CHAIN_HEADER_PREFIX, MAX_AUTH_CHAIN_LINKS);
    if header_str(headers, &overflow_name).is_some() {
        return Err(AuthChainError::MalformedChain {
            detail: format!("exceeds max length of {}", MAX_AUTH_CHAIN_LINKS),
        });
    }

    if links.len() < 2 {
        return Err(AuthChainError::InsufficientLinks);
    }

    let signer = links[0].payload.to_lowercase();
    Ok(AuthChain { links, signer })
}

/// Verify the auth chain's signature against the canonical payload.
///
/// Recovers the signer by walking the chain via `catalyrst-crypto`, then
/// enforces the timestamp window (`|now - signed_at| > expiration_secs` → Err).
/// Returns the verified root signer address (lowercased).
///
/// `payload` is the canonical
/// `"<method-lowercase>:<path-lowercase>:<timestamp>:<metadata>"` string that
/// the client's last (leaf) link signed. The verifier checks that the recovered
/// final authority equals the leaf-link payload (i.e. the canonical request
/// payload).
pub fn validate_signature(
    chain: &AuthChain,
    payload: &str,
    expiration_secs: i64,
    now: i64,
) -> Result<EthAddress, AuthChainError> {
    // Time-window check (in seconds). We extract the timestamp from the
    // canonical payload's 3rd colon-separated field. If callers pass a payload
    // that doesn't follow `m:p:ts:meta`, the check is skipped here (we still
    // verify the signature). Handlers should always build payloads via
    // `build_payload` to keep this in sync.
    if let Some(signed_at_ms) = payload
        .split(':')
        .nth(2)
        .and_then(|s| s.parse::<i64>().ok())
    {
        // Marketplace clients send millis-since-epoch in
        // `x-identity-timestamp`. Normalize to seconds before comparing.
        let signed_at = signed_at_ms / 1000;
        if (now - signed_at).abs() > expiration_secs {
            return Err(AuthChainError::Expired {
                signed_at,
                now,
                window_secs: expiration_secs,
            });
        }
    }

    // Project our struct back to catalyrst-types' shape for the crypto
    // verifier. We treat the leaf-link payload as the expected final
    // authority, matching `Authenticator.validateSignature` semantics.
    let crypto_chain: Vec<CryptoAuthLink> = chain
        .links
        .iter()
        .map(|link| CryptoAuthLink {
            link_type: match link.kind {
                AuthLinkType::Signer => CryptoAuthLinkType::SIGNER,
                AuthLinkType::EcdsaPersonalEphemeral => CryptoAuthLinkType::EcdsaEphemeral,
                AuthLinkType::EcdsaPersonalSignedEntity => CryptoAuthLinkType::EcdsaSignedEntity,
            },
            payload: link.payload.clone(),
            signature: if link.signature.is_empty() {
                None
            } else {
                Some(link.signature.clone())
            },
        })
        .collect();

    // The canonical "expected address" for the verifier is the leaf payload —
    // i.e. the last link signed exactly the request payload. That payload IS
    // the final authority.
    verify_auth_chain(&crypto_chain, payload, Some(now * 1000)).map_err(map_auth_error)?;

    Ok(chain.signer.clone())
}

fn map_auth_error(err: AuthError) -> AuthChainError {
    match err {
        AuthError::MalformedChain(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::MissingSignature { .. } => AuthChainError::MalformedChain {
            detail: err.to_string(),
        },
        AuthError::RecoveryFailed(d) => AuthChainError::InvalidSignature(d),
        AuthError::SignerMismatch { .. } => AuthChainError::InvalidSignature(err.to_string()),
        AuthError::FinalAuthorityMismatch { .. } => {
            AuthChainError::InvalidSignature(err.to_string())
        }
        AuthError::EphemeralExpired {
            expiration_ms,
            now_ms,
        } => AuthChainError::Expired {
            signed_at: expiration_ms / 1000,
            now: now_ms / 1000,
            window_secs: 0,
        },
        AuthError::InvalidEphemeralPayload(d) => AuthChainError::MalformedChain { detail: d },
        AuthError::Eip1654NotImplemented
        | AuthError::Eip1654ValidationFailed(_)
        | AuthError::Eip1654Rejected { .. } => AuthChainError::EipNotImplemented,
    }
}

/// Convenience for handlers: verify and check the `?address=` query param.
/// Returns the verified address; errors on mismatch.
pub fn verify_with_address(
    chain: &AuthChain,
    payload: &str,
    expiration_secs: i64,
    now: i64,
    expected_address: &str,
) -> Result<EthAddress, AuthChainError> {
    let recovered = validate_signature(chain, payload, expiration_secs, now)?;
    if recovered.to_lowercase() != expected_address.to_lowercase() {
        return Err(AuthChainError::AddressMismatch {
            expected: expected_address.to_lowercase(),
            recovered,
        });
    }
    Ok(recovered)
}
