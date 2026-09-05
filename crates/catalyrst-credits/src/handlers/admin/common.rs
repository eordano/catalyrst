use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};
use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::http::ApiError;
use crate::AppState;

/// The environment variable that names the credits admin bearer secret. Server-chosen; it
/// becomes the verified audit actor (`service-token:CATALYRST_CREDITS_ADMIN_TOKEN`) and the
/// name reported when the secret is unset. Never client-supplied.
const ADMIN_TOKEN_ENV: &str = "CATALYRST_CREDITS_ADMIN_TOKEN";

/// A credits-local carrier for the configured admin secret, so the shared extractor's
/// `ConfiguredAdminBearerSecret: FromRef<S>` bound is satisfied by a *local* concrete state
/// type. Implementing `FromRef` for the foreign [`ConfiguredAdminBearerSecret`] over the
/// foreign `Arc<AppStateInner>` router state directly is forbidden by the orphan rule; this
/// local type is the legal seam, rebuilt from `state.admin_token` on each request.
#[derive(Clone)]
struct AdminSecretState(ConfiguredAdminBearerSecret);

impl FromRef<AdminSecretState> for ConfiguredAdminBearerSecret {
    fn from_ref(state: &AdminSecretState) -> Self {
        state.0.clone()
    }
}

/// Proof, wired into a handler's *signature*, that this request carried the credits admin
/// bearer secret.
///
/// The old gate was `authorize_admin(&state, &headers)?` -- a forgettable body call: delete the
/// line and the handler still compiled and served a production mutation to a stranger.
/// `RequireAdmin` replaces that with a value every admin handler must *name in its signature*;
/// axum refuses the handler into `Router::route` unless the argument resolves, and the only way
/// it resolves is the [`FromRequestParts`] impl below, which runs the shared verified
/// [`AuthenticatedAdminIdentity`] mint. The check stops being a deletable statement and becomes a
/// term in the type the router demands. `tests/admin_routes_are_gated.rs` pins the residual gap --
/// a *new* admin route that forgets to name it.
///
/// The inner identity is a private tuple field: only this module can mint a `RequireAdmin`, and
/// only via [`establish_admin`]. It derives nothing -- no `Deserialize` (a request body must never
/// become an admin identity), no `Clone`/`Default` -- the same discipline as the shared type and
/// `catalyrst-server`'s `AdminSession`.
pub(crate) struct RequireAdmin(AuthenticatedAdminIdentity);

impl RequireAdmin {
    /// The server-verified audit actor, `service-token:CATALYRST_CREDITS_ADMIN_TOKEN`. Built by
    /// the principal crate from the `&'static str` the operator configured -- it replaces the old
    /// client-supplied `x-catalyrst-admin` header value, which the server never verified.
    pub(crate) fn audit_actor_description(&self) -> String {
        self.0.audit_actor_description()
    }
}

/// Preserve the pre-migration credits wire contract: every admin-auth failure renders as a
/// **403** carrying the `{ok:false,error,message}` envelope, with the same two messages the
/// deleted `authorize_with_token` produced -- the unset-token notice when the secret is not
/// configured, `"invalid admin token"` for a missing or mismatched bearer. This deliberately
/// collapses the shared extractor's 503-vs-401 distinction back to 403; adopting 401/503 is a
/// separate follow-on.
fn to_api_error(rejection: AdminAuthRejection) -> ApiError {
    match rejection.refusal() {
        AuthorityNotEstablished::CredentialNotConfigured { .. } => {
            ApiError::forbidden("admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)")
        }
        _ => ApiError::forbidden("invalid admin token"),
    }
}

/// The single mint for [`RequireAdmin`]: build the local secret carrier from the configured
/// token, run the shared verified extractor over the request parts, and map its rejection onto
/// the credits wire contract. Split out from the trait impl so it is unit-testable without a full
/// `AppState` (which would require a live database).
async fn establish_admin(
    configured: Option<String>,
    parts: &mut Parts,
) -> Result<RequireAdmin, ApiError> {
    let carrier = AdminSecretState(ConfiguredAdminBearerSecret {
        environment_variable: ADMIN_TOKEN_ENV,
        configured,
    });
    match AuthenticatedAdminIdentity::from_request_parts(parts, &carrier).await {
        Ok(identity) => Ok(RequireAdmin(identity)),
        Err(rejection) => Err(to_api_error(rejection)),
    }
}

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        establish_admin(state.admin_token.clone(), parts).await
    }
}

pub(super) fn validate_idempotency_key(raw: &Option<String>) -> Result<Option<String>, ApiError> {
    match raw {
        None => Ok(None),
        Some(k) => {
            let t = k.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 200 {
                Err(ApiError::bad_request("idempotencyKey too long (max 200)"))
            } else if !t.chars().all(|c| c.is_ascii_graphic()) {
                Err(ApiError::bad_request(
                    "idempotencyKey must be printable ASCII",
                ))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

pub(super) fn normalize_address(raw: &str) -> Result<String, ApiError> {
    catalyrst_types::normalize_eth_address(raw)
        .ok_or_else(|| ApiError::bad_request("invalid wallet address"))
}

pub(crate) fn validate_positive_amount(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 78 {
        return Err(ApiError::bad_request("invalid amount"));
    }
    let mut seen_dot = false;
    let mut any_digit = false;
    let mut any_nonzero = false;
    for c in s.chars() {
        match c {
            '0'..='9' => {
                any_digit = true;
                if c != '0' {
                    any_nonzero = true;
                }
            }
            '.' if !seen_dot => seen_dot = true,
            _ => return Err(ApiError::bad_request("invalid amount")),
        }
    }
    if !any_digit || !any_nonzero {
        return Err(ApiError::bad_request("amount must be a positive number"));
    }
    Ok(s.to_string())
}

pub(super) fn validated_reason(reason: &Option<String>) -> Result<Option<String>, ApiError> {
    match reason {
        None => Ok(None),
        Some(r) => {
            let t = r.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.len() > 500 {
                Err(ApiError::bad_request("reason too long (max 500)"))
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

pub(super) fn validate_sku(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 100 {
        return Err(ApiError::bad_request("invalid sku"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid sku"));
    }
    Ok(s.to_string())
}

pub(super) fn validate_escrow_ref(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 200 {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
    {
        return Err(ApiError::bad_request("invalid escrowRef"));
    }
    Ok(s.to_string())
}

pub(super) fn validate_price_cents(v: i64) -> Result<i64, ApiError> {
    if v < 0 {
        return Err(ApiError::bad_request("priceCents must be >= 0"));
    }
    Ok(v)
}

pub(super) fn validate_currency(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() || s.len() > 10 || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ApiError::bad_request("invalid currency"));
    }
    Ok(s)
}

pub(super) fn paginate(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;

    async fn parts_with_auth(authorization: Option<&str>) -> Parts {
        let mut builder = Request::builder();
        if let Some(value) = authorization {
            builder = builder.header("authorization", value);
        }
        let request = builder.body(()).expect("request builds");
        request.into_parts().0
    }

    async fn reject_status_and_body(
        configured: Option<&str>,
        authorization: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut parts = parts_with_auth(authorization).await;
        let error = match establish_admin(configured.map(str::to_string), &mut parts).await {
            Ok(_) => panic!("expected a rejection, not an established admin identity"),
            Err(error) => error,
        };
        let response = error.into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("body collects");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");
        (status, value)
    }

    #[tokio::test]
    async fn a_matching_bearer_yields_the_verified_service_token_actor() {
        let mut parts = parts_with_auth(Some("Bearer s3cret")).await;
        let admin = establish_admin(Some("s3cret".to_string()), &mut parts)
            .await
            .unwrap_or_else(|_| panic!("a matching secret establishes admin"));
        assert_eq!(
            admin.audit_actor_description(),
            "service-token:CATALYRST_CREDITS_ADMIN_TOKEN"
        );
    }

    // The pre-migration wire contract: 403 + the credits error envelope, with the same two
    // messages the deleted `authorize_with_token` produced, for every failure mode. These lock
    // the behaviour the old gate had rather than the shared extractor's native 401/503.
    #[tokio::test]
    async fn an_unset_token_fails_closed_as_403_disabled() {
        for presented in [Some("Bearer anything"), None] {
            let (status, body) = reject_status_and_body(None, presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["ok"], serde_json::json!(false));
            assert_eq!(
                body["error"],
                "admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)"
            );
            assert_eq!(
                body["message"],
                "admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)"
            );
        }
    }

    #[tokio::test]
    async fn an_empty_configured_token_also_reads_as_disabled() {
        let (status, body) = reject_status_and_body(Some(""), Some("Bearer ")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body["error"],
            "admin controls are disabled (CATALYRST_CREDITS_ADMIN_TOKEN unset)"
        );
    }

    #[tokio::test]
    async fn a_missing_wrong_or_unprefixed_bearer_is_403_invalid() {
        // The raw `"secret"` case pins that the exact `"Bearer "` prefix is still required.
        let cases = [
            None,
            Some("Bearer nope"),
            Some("Basic s3cret"),
            Some("secret"),
        ];
        for presented in cases {
            let (status, body) = reject_status_and_body(Some("secret"), presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["error"], "invalid admin token");
            assert_eq!(body["message"], "invalid admin token");
        }
    }

    #[tokio::test]
    async fn a_correct_bearer_authorizes() {
        let mut parts = parts_with_auth(Some("Bearer secret")).await;
        assert!(establish_admin(Some("secret".to_string()), &mut parts)
            .await
            .is_ok());
    }

    #[test]
    fn validates_address() {
        assert!(normalize_address("0x1234567890abcdef1234567890abcdef12345678").is_ok());
        assert_eq!(
            normalize_address("0xABCDEF1234567890ABCDEF1234567890ABCDEF12").unwrap(),
            "0xabcdef1234567890abcdef1234567890abcdef12"
        );
        assert!(normalize_address("notanaddress").is_err());
        assert!(normalize_address("0x123").is_err());
    }

    #[test]
    fn validates_positive_amount() {
        assert_eq!(validate_positive_amount("100").unwrap(), "100");
        assert_eq!(validate_positive_amount(" 12.5 ").unwrap(), "12.5");
        assert!(validate_positive_amount("0").is_err());
        assert!(validate_positive_amount("0.0").is_err());
        assert!(validate_positive_amount("-5").is_err());
        assert!(validate_positive_amount("1e9").is_err());
        assert!(validate_positive_amount("").is_err());
    }

    #[test]
    fn validates_idempotency_key() {
        assert_eq!(validate_idempotency_key(&None).unwrap(), None);
        assert_eq!(validate_idempotency_key(&Some("  ".into())).unwrap(), None);
        assert_eq!(
            validate_idempotency_key(&Some(" grant-2026-001 ".into())).unwrap(),
            Some("grant-2026-001".to_string())
        );
        assert!(validate_idempotency_key(&Some("x".repeat(201))).is_err());
        assert!(validate_idempotency_key(&Some("bad key".into())).is_err());
        assert!(validate_idempotency_key(&Some("bad\nkey".into())).is_err());
    }

    #[test]
    fn validates_sku_phase8() {
        assert_eq!(validate_sku(" pack_100 ").unwrap(), "pack_100");
        assert!(validate_sku("").is_err());
        assert!(validate_sku("a/b").is_err());
        assert!(validate_sku("a\\b").is_err());
        assert!(validate_sku(&"x".repeat(101)).is_err());
    }

    #[test]
    fn validates_escrow_ref() {
        assert_eq!(validate_escrow_ref(" 0xdeadBEEF ").unwrap(), "0xdeadBEEF");
        assert!(validate_escrow_ref("").is_err());
        assert!(validate_escrow_ref("a/b").is_err());
        assert!(validate_escrow_ref(&"x".repeat(201)).is_err());
    }

    #[test]
    fn validates_price_cents() {
        assert_eq!(validate_price_cents(0).unwrap(), 0);
        assert_eq!(validate_price_cents(999).unwrap(), 999);
        assert!(validate_price_cents(-1).is_err());
    }

    #[test]
    fn validates_currency() {
        assert_eq!(validate_currency(" USD ").unwrap(), "usd");
        assert_eq!(validate_currency("eur").unwrap(), "eur");
        assert!(validate_currency("").is_err());
        assert!(validate_currency("us1").is_err());
        assert!(validate_currency(&"a".repeat(11)).is_err());
    }

    #[test]
    fn paginates_with_bounds() {
        assert_eq!(paginate(None, None), (50, 0));
        assert_eq!(paginate(Some(10), Some(5)), (10, 5));
        assert_eq!(paginate(Some(0), Some(-3)), (1, 0));
        assert_eq!(paginate(Some(9999), None), (200, 0));
    }
}
