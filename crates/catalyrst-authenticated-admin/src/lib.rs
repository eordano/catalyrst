//! An unforgeable axum extractor for the workspace's static-bearer admin gate.
//!
//! # What this crate is
//!
//! The axum-facing companion to [`catalyrst_authenticated_principal`]. That crate is
//! deliberately I/O-free vocabulary -- its own `tests/source_discipline.rs` forbids `axum`,
//! `sqlx`, `tokio` and friends -- so a [`FromRequestParts`] impl cannot live there. This
//! crate is the one place that impl lives, and it does nothing but wire axum's request
//! plumbing onto the verifier that already exists:
//! [`establish_platform_service_identity_by_comparing_presented_shared_secret`].
//!
//! # The defect it closes
//!
//! Four crates (`catalyrst-badges`, `-economy`, `-credits`, `-telemetry`) each hand-roll a
//! `require_admin()` / `authorize_admin()` gate that is a *forgettable function call* made
//! inside the handler body. Delete the call and the handler still compiles and serves a
//! production mutation to a stranger. [`AuthenticatedAdminIdentity`] replaces that pattern
//! with a value that a handler must *name in its signature*: axum will not accept a handler
//! into `Router::route` unless every argument is a valid extractor, and this type's only
//! constructor is the [`FromRequestParts`] impl below, which runs the bearer check. The
//! check stops being a statement that can be dropped and becomes a term in the type the
//! router demands.
//!
//! This is the same model `catalyrst-server` already uses for its SIWE console
//! (`AdminSession`): a private field, one construction site, and a `source_discipline` test
//! that pins both. See `docs/auth-arc-plan.md`.
//!
//! # What a value of [`AuthenticatedAdminIdentity`] proves -- and does not
//!
//! That the request presented the operator-configured admin bearer secret for this service.
//! That is a *service credential*, not a person and not a wallet -- exactly the
//! [`AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken`] it wraps. It says a
//! service called; it never says the service may act.
//!
//! # This pass builds only the crate
//!
//! No consumer is migrated here. Each adopting crate provides
//! [`ConfiguredAdminBearerSecret`] to the extractor through axum's [`FromRef`] over its own
//! `AppState`, and swaps its `require_admin()` body call for an
//! [`AuthenticatedAdminIdentity`] argument. That is the Pilot phase and is out of scope for
//! this crate landing.

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
/// The inner `principal` is deliberately private, and it is always the
/// [`AuthenticatedPrincipal::PlatformServiceProvenBySharedBearerToken`] variant. A public
/// field -- or any second constructor -- would let a handler mint one from a bare value and
/// hand it to a gate, which is precisely the forgeable `require_admin()` this type exists to
/// replace. The only construction path is the [`FromRequestParts`] impl below;
/// `tests/source_discipline.rs` pins that as a fact about the source.
///
/// It derives nothing on purpose. No `Deserialize` (a request body must never become an
/// admin identity), no `Clone`/`Default` (they widen how a value comes to exist) -- the same
/// discipline as `catalyrst-server`'s `AdminSession`.
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
/// Each adopting crate constructs one of these in its [`FromRef`] impl, reading the token
/// out of wherever its own `AppState` keeps it (`state.admin_token`,
/// `state.config.admin_token`, ...). It carries no verification of its own -- it is just the
/// expected secret handed to the extractor, which compares it in constant time via the
/// principal-crate chokepoint.
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
/// Wraps the principal crate's [`AuthorityNotEstablished`], whose `http_status()` already
/// draws the distinctions this arc adopts: **401** for a missing or mismatched secret,
/// **503** for an unconfigured one. Every adopting crate gets identical status semantics for
/// free, without teaching its own `ApiError` about admin auth. Adopting crates that want to
/// fold this into their own error type can read the inner refusal via [`Self::refusal`].
///
/// > Behaviour change to flag: the four current gates all return **403** for both an unset
/// > token and a bad/missing token. This type returns **503** (unconfigured) vs **401**
/// > (missing/mismatch) instead -- the deliberate semantics of the principal crate. Note it
/// > in each crate's migration PR.
pub struct AdminAuthRejection(AuthorityNotEstablished);

impl AdminAuthRejection {
    /// The underlying refusal, for adopting crates that want to map it onto their own error
    /// type instead of using the built-in [`IntoResponse`].
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
        // A fixed, server-authored body keyed on the status class. Deliberately generic: it
        // must not echo the operator-facing detail (which can name the environment variable
        // behind a 503) back to the client.
        let body = match status {
            StatusCode::UNAUTHORIZED => "admin authentication required",
            StatusCode::SERVICE_UNAVAILABLE => "admin authentication unavailable",
            _ => "forbidden",
        };
        (status, body).into_response()
    }
}

/// Parse the bearer token out of the `Authorization` header, requiring the exact `"Bearer "`
/// prefix.
///
/// The principal crate refuses to parse the header itself -- twenty of twenty-one gates in
/// the workspace require this exact prefix, one accepts a lowercase variant, and widening
/// the shared verifier would loosen all twenty at once. So the lone piece of header parsing
/// lives here, matching the twenty-gate majority.
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

    // Neither AuthenticatedAdminIdentity nor AdminAuthRejection derives Debug (the identity
    // must derive nothing; the source-discipline test pins that), so these unwrap with a
    // `match` rather than `.expect()` / `.expect_err()`, which would demand Debug on the
    // other arm.
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
