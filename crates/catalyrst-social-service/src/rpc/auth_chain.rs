use axum::http::HeaderMap;

use catalyrst_crypto::signed_fetch::handshake;
use catalyrst_crypto::Signer;

pub use catalyrst_crypto::signed_fetch::handshake::{
    extract_from_object, validate_signature, AuthChainError,
};
pub use catalyrst_crypto::signed_fetch::{
    build_payload, AuthChain, AuthLink, AUTH_CHAIN_HEADER_PREFIX, AUTH_METADATA_HEADER,
    AUTH_TIMESTAMP_HEADER, MAX_AUTH_CHAIN_LINKS,
};

pub const FIVE_MINUTES_SECS: i64 = 5 * 60;

/// No canonical metadata keys, so the pre-6.0.0 folded payload is never accepted on this socket
/// either: the 6.x payload binds the metadata bytes, which is what stops a re-cased metadata key
/// from folding back into the scene metadata that was actually signed. The explorer signs `{}` on
/// this handshake, which folds to itself, so the two payload shapes agree for real traffic.
const CANONICAL_METADATA_KEYS: &[&str] = &[];

pub async fn require_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<Signer, AuthChainError> {
    // ADR-44: refuse a scene acting as a user's identity, matching the HTTP routes (upstream #440).
    // The metadata gate answers before verification (upstream #492), so a refused request pays for
    // no crypto. This module's error enum has no dedicated variant, so the refusal reuses
    // InvalidSignature -- the WS handshake path collapses every handshake failure to a single
    // "Unauthorized" close, so the variant is not observable there.
    if headers_declare_refused_signer(headers) {
        return Err(AuthChainError::InvalidSignature(
            "requests from scenes are not allowed".to_string(),
        ));
    }
    handshake::require_signer_v6(
        headers,
        method,
        path,
        FIVE_MINUTES_SECS,
        CANONICAL_METADATA_KEYS,
        None,
    )
    .await
}

/// The gate is passed as `None`: the crate-local pre-gates above and in [`ws`] answer first and own
/// the refusal text this crate logs.
///
/// [`ws`]: crate::rpc::ws
pub async fn verify_handshake(
    frame_json: &str,
    method: &str,
    path: &str,
    expiration_secs: i64,
    now_secs: i64,
) -> Result<Signer, AuthChainError> {
    handshake::verify_handshake_v6(
        frame_json,
        method,
        path,
        expiration_secs,
        now_secs,
        CANONICAL_METADATA_KEYS,
        None,
    )
    .await
}

/// Whether the `x-identity-metadata` header of a signed-fetch request carries a refused signer.
fn headers_declare_refused_signer(headers: &HeaderMap) -> bool {
    let Some(raw) = headers
        .get(AUTH_METADATA_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let metadata: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
    crate::scene_signer::is_refused_signer(&metadata)
}

/// Whether a WS auth-handshake frame carries a refused signer. The frame is the header bag encoded
/// as one JSON object whose `x-identity-metadata` value is itself a JSON string, so it is parsed in
/// two steps. Used by the WS handshake to refuse scene-signed chains (ADR-44, upstream #440).
pub fn frame_declares_refused_signer(frame_json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(frame_json) else {
        return false;
    };
    let Some(obj) = value.as_object() else {
        return false;
    };
    let meta_raw = handshake::obj_str(obj, AUTH_METADATA_HEADER).unwrap_or("{}");
    let metadata: serde_json::Value =
        serde_json::from_str(meta_raw).unwrap_or(serde_json::Value::Null);
    crate::scene_signer::is_refused_signer(&metadata)
}

pub fn handshake_path(headers: &HeaderMap, fallback: &str) -> String {
    match headers.get("x-original-path").and_then(|v| v.to_str().ok()) {
        Some(raw) => {
            let stripped = raw.split('?').next().unwrap_or(raw);
            let accept = if fallback == "/" {
                let seg = stripped.trim_end_matches('/');
                seg.starts_with('/') && seg.len() > 1 && !seg[1..].contains('/')
            } else {
                stripped.ends_with(fallback)
            };
            if accept {
                stripped.to_string()
            } else {
                fallback.to_string()
            }
        }
        None => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::signers::{local::PrivateKeySigner, Signer};

    async fn make_chain(method: &str, path: &str, ts_ms: i64) -> (String, String) {
        let payload = build_payload(method, path, &ts_ms.to_string(), "{}");
        make_frame(ts_ms, &payload, "{}").await
    }

    /// A genuinely signed frame whose delivered metadata may differ from the metadata that went into
    /// the signed payload -- the shape of the re-cased-key attack, where nothing about the signature
    /// is weakened and only the bytes on the wire are rewritten.
    async fn make_frame(
        ts_ms: i64,
        signed_payload: &str,
        delivered_metadata: &str,
    ) -> (String, String) {
        let root: PrivateKeySigner =
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .unwrap();
        let root_address = format!("{:#x}", root.address());

        let ephemeral: PrivateKeySigner =
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
                .parse()
                .unwrap();
        let ephemeral_address = format!("{:#x}", ephemeral.address());

        let ephemeral_payload = format!(
            "Decentraland Login\nEphemeral address: {}\nExpiration: 2099-01-01T00:00:00.000Z",
            ephemeral_address
        );
        let ephemeral_sig = root
            .sign_message(ephemeral_payload.as_bytes())
            .await
            .unwrap();

        let entity_sig = ephemeral
            .sign_message(signed_payload.as_bytes())
            .await
            .unwrap();

        let frame = serde_json::json!({
            "x-identity-auth-chain-0": serde_json::json!({
                "type": "SIGNER",
                "payload": root_address,
                "signature": ""
            }).to_string(),
            "x-identity-auth-chain-1": serde_json::json!({
                "type": "ECDSA_EPHEMERAL",
                "payload": ephemeral_payload,
                "signature": ephemeral_sig.to_string()
            }).to_string(),
            "x-identity-auth-chain-2": serde_json::json!({
                "type": "ECDSA_SIGNED_ENTITY",
                "payload": signed_payload,
                "signature": entity_sig.to_string()
            }).to_string(),
            "x-identity-timestamp": ts_ms.to_string(),
            "x-identity-metadata": delivered_metadata
        });
        (root_address, frame.to_string())
    }

    #[tokio::test]
    async fn verify_handshake_accepts_valid_chain() {
        let now_secs = 1_700_000_000;
        let (expected_signer, frame) = make_chain("get", "/", now_secs * 1000).await;
        let signer = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect("valid chain must verify");
        assert_eq!(signer, expected_signer.to_lowercase());
    }

    #[tokio::test]
    async fn verify_handshake_rejects_method_path_mismatch() {
        let now_secs = 1_700_000_000;
        let (_, frame) = make_chain("get", "/", now_secs * 1000).await;
        let err = verify_handshake(&frame, "post", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect_err("wrong method must be rejected");
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn verify_handshake_rejects_expired() {
        let signed_secs = 1_700_000_000;
        let now_secs = signed_secs + 10 * 60;
        let (_, frame) = make_chain("get", "/", signed_secs * 1000).await;
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect_err("expired chain must be rejected");
        assert!(matches!(err, AuthChainError::Expired { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn verify_handshake_accepts_prefixed_effective_path() {
        let now_secs = 1_700_000_000;
        let (expected_signer, frame) = make_chain("get", "/social-rpc", now_secs * 1000).await;
        let signer = verify_handshake(&frame, "get", "/social-rpc", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect("prefixed-path chain must verify against matching effective path");
        assert_eq!(signer, expected_signer.to_lowercase());
    }

    #[tokio::test]
    async fn verify_handshake_rejects_prefixed_frame_against_root() {
        let now_secs = 1_700_000_000;
        let (_, frame) = make_chain("get", "/social-rpc", now_secs * 1000).await;
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect_err("prefixed-path chain must not verify against /");
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
    }

    #[test]
    fn handshake_path_accepts_stripped_prefix_and_defaults_to_root() {
        use axum::http::HeaderValue;
        let mut headers = HeaderMap::new();
        headers.insert("x-original-path", HeaderValue::from_static("/social-rpc"));
        assert_eq!(handshake_path(&headers, "/"), "/social-rpc");
        headers.insert(
            "x-original-path",
            HeaderValue::from_static("/social-rpc/?ts=1"),
        );
        assert_eq!(handshake_path(&headers, "/"), "/social-rpc/");
        assert_eq!(handshake_path(&HeaderMap::new(), "/"), "/");
    }

    #[test]
    fn handshake_path_rejects_multi_segment_original_against_root() {
        use axum::http::HeaderValue;
        let mut headers = HeaderMap::new();
        headers.insert("x-original-path", HeaderValue::from_static("/v1/friends"));
        assert_eq!(handshake_path(&headers, "/"), "/");
        headers.insert(
            "x-original-path",
            HeaderValue::from_static("/social-rpc/deep"),
        );
        assert_eq!(handshake_path(&headers, "/"), "/");
    }

    #[test]
    fn handshake_path_rejects_non_suffix_original() {
        use axum::http::HeaderValue;
        let mut headers = HeaderMap::new();
        headers.insert("x-original-path", HeaderValue::from_static("/v1/friends"));
        assert_eq!(
            handshake_path(&headers, "/v1/communities"),
            "/v1/communities"
        );
        headers.insert(
            "x-original-path",
            HeaderValue::from_static("/edge/v1/lists"),
        );
        assert_eq!(handshake_path(&headers, "/v1/lists"), "/edge/v1/lists");
    }

    #[tokio::test]
    async fn verify_handshake_rejects_malformed_envelope() {
        let err = verify_handshake("not json", "get", "/", FIVE_MINUTES_SECS, 0)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::MalformedChain { .. }),
            "{err:?}"
        );
        let err2 = verify_handshake("[]", "get", "/", FIVE_MINUTES_SECS, 0)
            .await
            .unwrap_err();
        assert!(
            matches!(err2, AuthChainError::EnvelopeNotObject),
            "{err2:?}"
        );
    }

    #[tokio::test]
    async fn verify_handshake_rejects_short_chain() {
        let frame = serde_json::json!({
            "x-identity-auth-chain-0": "{\"type\":\"SIGNER\",\"payload\":\"0xabc\",\"signature\":\"\"}",
            "x-identity-timestamp": "0",
            "x-identity-metadata": "{}"
        })
        .to_string();
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, 0)
            .await
            .unwrap_err();
        assert!(matches!(err, AuthChainError::InsufficientLinks), "{err:?}");
    }

    #[test]
    fn frame_declares_refused_signer_detects_scene_metadata() {
        // The frame's x-identity-metadata value is itself a JSON string (upstream #440).
        let scene = serde_json::json!({
            "x-identity-metadata":
                serde_json::json!({ "signer": "decentraland-kernel-scene" }).to_string()
        })
        .to_string();
        assert!(frame_declares_refused_signer(&scene));

        let recased = serde_json::json!({
            "x-identity-metadata":
                serde_json::json!({ "signer": "Decentraland-Kernel-Scene" }).to_string()
        })
        .to_string();
        assert!(frame_declares_refused_signer(&recased));

        // The empty metadata the explorer sends on this socket, and other signers, pass.
        assert!(!frame_declares_refused_signer(
            &serde_json::json!({ "x-identity-metadata": "{}" }).to_string()
        ));
        assert!(!frame_declares_refused_signer(
            &serde_json::json!({
                "x-identity-metadata": serde_json::json!({ "signer": "dcl:explorer" }).to_string()
            })
            .to_string()
        ));
        // Absent metadata and a malformed frame must not panic and must not match.
        assert!(!frame_declares_refused_signer("{}"));
        assert!(!frame_declares_refused_signer("not json"));
    }

    #[test]
    fn frame_declares_refused_signer_refuses_a_non_string_signer() {
        // Upstream #492: a signer that is not a canonical string is refused, not waved through as
        // ordinary user traffic.
        let typed = serde_json::json!({
            "x-identity-metadata": serde_json::json!({ "signer": 42 }).to_string()
        })
        .to_string();
        assert!(frame_declares_refused_signer(&typed));
    }

    const SCENE_METADATA: &str = r#"{"signer":"decentraland-kernel-scene"}"#;
    const RECASED_KEY: &str = r#"{"Signer":"decentraland-kernel-scene"}"#;

    #[tokio::test]
    async fn a_re_cased_metadata_key_no_longer_folds_back_into_the_signed_scene_metadata() {
        let now_secs = 1_700_000_000;
        let ts_ms = now_secs * 1000;
        let signed = build_payload("get", "/", &ts_ms.to_string(), SCENE_METADATA);
        let (_, frame) = make_frame(ts_ms, &signed, RECASED_KEY).await;

        // The pre-gate reads `signer` and nothing else, so it passes this frame on; what refuses it
        // is the signature, which now binds the metadata bytes.
        assert!(!frame_declares_refused_signer(&frame));
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .expect_err("a scene-signed chain must not authenticate as a user");
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn a_metadata_field_this_service_never_reads_is_still_bound_to_the_signature() {
        let now_secs = 1_700_000_000;
        let ts_ms = now_secs * 1000;
        let signed = build_payload(
            "get",
            "/",
            &ts_ms.to_string(),
            r#"{"sceneId":"bafkreiabcdef"}"#,
        );
        let (_, frame) = make_frame(ts_ms, &signed, r#"{"sceneId":"BAFKREIABCDEF"}"#).await;
        let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthChainError::InvalidSignature(_)),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn unparseable_metadata_is_refused_as_malformed() {
        let now_secs = 1_700_000_000;
        let ts_ms = now_secs * 1000;
        for raw in ["not json", "[]", "42"] {
            let signed = build_payload("get", "/", &ts_ms.to_string(), raw);
            let (_, frame) = make_frame(ts_ms, &signed, raw).await;
            let err = verify_handshake(&frame, "get", "/", FIVE_MINUTES_SECS, now_secs)
                .await
                .unwrap_err();
            assert!(
                matches!(err, AuthChainError::MalformedChain { .. }),
                "{raw}: {err:?}"
            );
        }
    }
}
