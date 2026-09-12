//! An unforgeable axum extractor for the workspace's static-bearer admin gate.
//!
//! The axum-facing companion to [`catalyrst_authenticated_principal`], which is deliberately
//! I/O-free vocabulary -- its own `tests/source_discipline.rs` forbids `axum`, `sqlx` and
//! `tokio` -- so a [`FromRequestParts`] impl cannot live there. This crate is that impl and
//! nothing else; the verification is
//! [`establish_platform_service_identity_by_comparing_presented_shared_secret`].
//!
//! The defect it closes: `catalyrst-badges`, `-economy`, `-credits` and `-telemetry` each
//! hand-roll a `require_admin()` / `authorize_admin()` gate that is a *forgettable function
//! call* inside the handler body -- delete it and the handler still compiles and serves a
//! production mutation to a stranger. [`AuthenticatedAdminIdentity`] must instead be *named in
//! the handler signature*, and axum refuses a handler whose arguments are not extractors, so
//! the check stops being a statement that can be dropped. Same model `catalyrst-server`
//! already uses for its SIWE console (`AdminSession`): a private field, one construction site,
//! and a `source_discipline` test pinning both. See `docs/auth-arc-plan.md`.
//!
//! A value proves the request presented the operator-configured admin bearer secret for this
//! service -- a *service credential*, not a person and not a wallet. It says a service called;
//! it never says the service may act.
//!
//! No consumer is migrated here: an adopting crate provides [`ConfiguredAdminBearerSecret`]
//! through axum's [`FromRef`] over its own `AppState` and swaps its `require_admin()` body
//! call for an [`AuthenticatedAdminIdentity`] argument.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use catalyrst_authenticated_principal::{
    establish_platform_service_identity_by_comparing_presented_shared_secret,
    AuthenticatedPrincipal, AuthorityNotEstablished,
};

/// Proof that this request carried the operator-configured admin bearer secret for the
/// service reached through the state `S`.
///
/// The private `principal` is always the
/// [`AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken`] variant. A public
/// field, or any second constructor, would let a handler mint one from a bare value -- exactly
/// the forgeable `require_admin()` this replaces -- so the [`FromRequestParts`] impl below is
/// the only construction path, pinned by `tests/source_discipline.rs`. It derives nothing:
/// `Deserialize` would let a request body become an admin identity, and `Clone`/`Default`
/// widen how a value comes to exist.
pub struct AuthenticatedAdminIdentity {
    principal: AuthenticatedPrincipal,
}

impl AuthenticatedAdminIdentity {
    /// The verified principal behind this admin identity -- always the
    /// [`AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken`] variant.
    pub fn principal(&self) -> &AuthenticatedPrincipal {
        &self.principal
    }

    /// A server-verified audit actor, of the form
    /// `service-token:CATALYRST_X_ADMIN_TOKEN`. Built by the principal crate from the
    /// `&'static str` the operator configured -- never from client-supplied text such as the
    /// old `x-catalyrst-admin` header. Not a stable wire format; do not parse it.
    pub fn audit_actor_description(&self) -> String {
        self.principal.audit_actor_description()
    }
}

/// The operator-configured admin secret and the environment variable that named it.
///
/// Each adopting crate constructs one in its [`FromRef`] impl from wherever its own `AppState`
/// keeps the token. It verifies nothing itself: it is the expected secret handed to the
/// extractor, which compares it in constant time via the principal-crate chokepoint.
#[derive(Clone)]
pub struct ConfiguredAdminBearerSecret {
    /// The environment variable that named this credential, e.g.
    /// `"CATALYRST_BADGES_ADMIN_TOKEN"`. Server-chosen; it becomes the audit actor and the
    /// 503 message when the secret is unset. Never client-supplied.
    pub environment_variable: &'static str,
    /// The configured secret, or `None`/empty when the operator has not set it -- in which
    /// case the gate fails closed with a 503 (a deployment fault, not a denial).
    pub configured: Option<String>,
}

/// The rejection returned when admin authentication is not established.
///
/// Wraps the principal crate's [`AuthorityNotEstablished`], whose `http_status()` is **401**
/// for a missing or mismatched secret and **503** for an unconfigured one. Adopting crates
/// that want their own error type can read the inner refusal via [`Self::refusal`].
///
/// > Behaviour change to flag: the four current gates all return **403** for both an unset
/// > token and a bad/missing token. Note it in each crate's migration PR.
pub struct AdminAuthRejection(AuthorityNotEstablished);

impl AdminAuthRejection {
    /// For adopting crates mapping it onto their own error type instead of [`IntoResponse`].
    pub fn refusal(&self) -> &AuthorityNotEstablished {
        &self.0
    }
}

impl From<AuthorityNotEstablished> for AdminAuthRejection {
    fn from(refusal: AuthorityNotEstablished) -> Self {
        Self(refusal)
    }
}

impl IntoResponse for AdminAuthRejection {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = match status {
            StatusCode::UNAUTHORIZED => "admin authentication required",
            StatusCode::SERVICE_UNAVAILABLE => "admin authentication unavailable",
            _ => "forbidden",
        };
        (status, body).into_response()
    }
}

/// Requires the exact `"Bearer "` prefix.
///
/// The principal crate refuses to parse the header itself: twenty of the workspace's
/// twenty-one gates require this exact prefix and one accepts a lowercase variant, so
/// widening the shared verifier would loosen all twenty at once. The lone piece of header
/// parsing lives here, matching the twenty-gate majority.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .map(str::to_string)
}

impl<S> FromRequestParts<S> for AuthenticatedAdminIdentity
where
    S: Send + Sync,
    ConfiguredAdminBearerSecret: FromRef<S>,
{
    type Rejection = AdminAuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let secret = ConfiguredAdminBearerSecret::from_ref(state);
        let presented = bearer_token(&parts.headers);
        let identity = establish_platform_service_identity_by_comparing_presented_shared_secret(
            secret.environment_variable,
            secret.configured.as_deref(),
            presented.as_deref(),
        )?;
        Ok(AuthenticatedAdminIdentity {
            principal: AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(identity),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    const VAR: &str = "CATALYRST_BADGES_ADMIN_TOKEN";

    #[derive(Clone)]
    struct TestState {
        secret: ConfiguredAdminBearerSecret,
    }

    impl FromRef<TestState> for ConfiguredAdminBearerSecret {
        fn from_ref(state: &TestState) -> Self {
            state.secret.clone()
        }
    }

    fn state(configured: Option<&str>) -> TestState {
        TestState {
            secret: ConfiguredAdminBearerSecret {
                environment_variable: VAR,
                configured: configured.map(str::to_string),
            },
        }
    }

    async fn extract(
        state: &TestState,
        authorization: Option<&str>,
    ) -> Result<AuthenticatedAdminIdentity, AdminAuthRejection> {
        let mut builder = Request::builder();
        if let Some(value) = authorization {
            builder = builder.header("authorization", value);
        }
        let request = builder.body(()).expect("request builds");
        let (mut parts, ()) = request.into_parts();
        AuthenticatedAdminIdentity::from_request_parts(&mut parts, state).await
    }

    async fn expect_identity(state: &TestState, auth: Option<&str>) -> AuthenticatedAdminIdentity {
        match extract(state, auth).await {
            Ok(identity) => identity,
            Err(_) => panic!("expected an established admin identity"),
        }
    }

    async fn expect_rejection(state: &TestState, auth: Option<&str>) -> AdminAuthRejection {
        match extract(state, auth).await {
            Ok(_) => panic!("expected a rejection, not an established identity"),
            Err(rejection) => rejection,
        }
    }

    #[tokio::test]
    async fn a_matching_bearer_secret_yields_a_service_token_actor() {
        let identity = expect_identity(&state(Some("s3cret")), Some("Bearer s3cret")).await;
        assert_eq!(
            identity.audit_actor_description(),
            format!("service-token:{VAR}")
        );
        assert!(matches!(
            identity.principal(),
            AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken(_)
        ));
    }

    #[tokio::test]
    async fn a_missing_bearer_is_401() {
        let rejection = expect_rejection(&state(Some("s3cret")), None).await;
        assert_eq!(rejection.refusal().http_status(), 401);
        assert_eq!(rejection.into_response().status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_bearer_is_401() {
        let rejection = expect_rejection(&state(Some("s3cret")), Some("Bearer wrong")).await;
        assert_eq!(rejection.refusal().http_status(), 401);
    }

    #[tokio::test]
    async fn a_non_bearer_scheme_is_401() {
        let rejection = expect_rejection(&state(Some("s3cret")), Some("Basic s3cret")).await;
        assert_eq!(rejection.refusal().http_status(), 401);
    }

    #[tokio::test]
    async fn an_unconfigured_secret_is_503_not_a_denial() {
        let rejection = expect_rejection(&state(None), Some("Bearer anything")).await;
        assert_eq!(rejection.refusal().http_status(), 503);
        assert_eq!(
            rejection.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn an_empty_configured_secret_counts_as_unconfigured() {
        let rejection = expect_rejection(&state(Some("")), Some("Bearer ")).await;
        assert_eq!(rejection.refusal().http_status(), 503);
    }

    #[test]
    fn bearer_token_requires_the_exact_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer xyz".parse().unwrap());
        assert_eq!(bearer_token(&headers).as_deref(), Some("xyz"));

        headers.insert("authorization", "bearer xyz".parse().unwrap());
        assert_eq!(bearer_token(&headers), None);

        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }
}
