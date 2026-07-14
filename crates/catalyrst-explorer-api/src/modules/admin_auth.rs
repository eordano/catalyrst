use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::env;

use catalyrst_authenticated_principal::{
    establish_platform_service_identity_by_comparing_presented_shared_secret,
    AuthorityNotEstablished,
};

pub const ADMIN_TOKEN_ENV: &str = "CATALYRST_EXPLORER_API_ADMIN_TOKEN";

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Map the shared verifier's refusal back onto this crate's historical wire response, so the
/// migration onto `catalyrst-authenticated-principal`'s constant-time comparison changes no
/// status code or body a client can observe. Same pattern as
/// `catalyrst-telemetry`'s `admin_rejection_as_legacy_forbidden`.
///
/// The shared verifier only ever returns `CredentialNotConfigured` (503) or one of
/// `AuthenticationMissingOrInvalid` / `PresentedSharedSecretDidNotMatch` (401) for this
/// comparison -- never `RefusedLacksAuthority` or `UndeterminedStoreUnavailable` -- so matching
/// on `http_status()` here is exhaustive in practice for every refusal this function can
/// produce.
fn admin_rejection_as_legacy_forbidden(refusal: &AuthorityNotEstablished) -> Response {
    match refusal.http_status() {
        503 => forbidden("admin token not configured"),
        _ => forbidden("invalid or missing bearer token"),
    }
}

pub fn require_admin(headers: &HeaderMap) -> Result<(), Response> {
    let configured = env::var(ADMIN_TOKEN_ENV).ok();
    let presented = bearer_token(headers);
    establish_platform_service_identity_by_comparing_presented_shared_secret(
        ADMIN_TOKEN_ENV,
        configured.as_deref(),
        presented.as_deref(),
    )
    .map(|_identity| ())
    .map_err(|refusal| admin_rejection_as_legacy_forbidden(&refusal))
}

fn forbidden(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "forbidden", "message": msg })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_bearer(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
        headers
    }

    async fn body_text(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    // Serializes every test in this module: `require_admin` reads the real process
    // environment variable directly (matching its pre-migration behaviour byte for byte), so
    // concurrent tests mutating `ADMIN_TOKEN_ENV` would race. `std::sync::Mutex` rather than
    // anything async since these are plain `#[test]`s.
    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[tokio::test]
    async fn unset_token_and_bad_token_return_the_exact_same_403_as_before_the_migration() {
        // Every env mutation and `require_admin` call happens under the guard;
        // the body reads below only touch already-materialized responses, so
        // the guard drops before any await (clippy::await_holding_lock).
        let (unset_resp, missing_resp, wrong_resp) = {
            let _guard = env_lock().lock().unwrap();

            // Unset-token case: the historical 403 body names the unset env var.
            std::env::remove_var(ADMIN_TOKEN_ENV);
            let unset_resp = require_admin(&HeaderMap::new()).expect_err("unset token must refuse");

            std::env::set_var(ADMIN_TOKEN_ENV, "s3cret");
            let missing_resp =
                require_admin(&HeaderMap::new()).expect_err("missing bearer must refuse");
            let wrong_resp = require_admin(&headers_with_bearer("nope"))
                .expect_err("mismatched bearer must refuse");

            assert!(require_admin(&headers_with_bearer("s3cret")).is_ok());

            std::env::remove_var(ADMIN_TOKEN_ENV);
            (unset_resp, missing_resp, wrong_resp)
        };

        assert_eq!(unset_resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body_text(unset_resp).await,
            r#"{"error":"forbidden","message":"admin token not configured"}"#
        );

        assert_eq!(missing_resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body_text(missing_resp).await,
            r#"{"error":"forbidden","message":"invalid or missing bearer token"}"#
        );

        assert_eq!(wrong_resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body_text(wrong_resp).await,
            r#"{"error":"forbidden","message":"invalid or missing bearer token"}"#
        );
    }
}
