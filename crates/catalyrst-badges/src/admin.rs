//! Compile-forced admin authentication for the badges mutation endpoints.
//!
//! The old gate here was a hand-rolled `authorize_admin(&state, &headers)?` -- a forgettable
//! function call made inside each handler body. Delete the line and the handler still
//! compiled and served a production mutation to a stranger. This module replaces that with
//! [`RequireAdmin`], a value a handler must *name in its signature*: axum refuses a handler
//! into `Router::route` unless every argument is a valid extractor, and `RequireAdmin`'s only
//! constructor is the [`FromRequestParts`] impl below, which delegates to the shared, verified
//! [`AuthenticatedAdminIdentity`] mint. The check stops being a deletable statement and
//! becomes a term in the type the router demands. `tests/admin_routes_are_gated.rs` pins that.
//!
//! # Why a badges-local wrapper rather than `AuthenticatedAdminIdentity` directly
//!
//! Two reasons, both structural:
//!
//! 1. **Wire preservation.** The shared extractor rejects unconfigured/missing/mismatched
//!    secrets with the principal crate's 503/401 statuses and a plain-text body. Badges must
//!    keep its pre-migration contract byte-for-byte: **403** carrying the
//!    `{ok:false,error,message}` envelope for *every* auth failure, exactly as the deleted
//!    `authorize_admin` returned. [`to_api_error`] maps the shared rejection back onto that
//!    contract. Adopting the 401/503 distinction is a deliberate, separate follow-on.
//! 2. **The orphan rule.** The shared extractor's bound is
//!    `ConfiguredAdminBearerSecret: FromRef<S>`. Badges' router state is
//!    `Arc<AppStateInner>`; implementing `FromRef` for the *foreign*
//!    `ConfiguredAdminBearerSecret` over that foreign `Arc` state is orphan-forbidden. A
//!    badges-local carrier ([`AdminSecretState`]) is the legal bridge, and the wrapper builds
//!    it from `state.admin_token` on each request.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};
use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::http::errors::ApiError;
use crate::AppState;

/// The environment variable that names the badges admin bearer secret. Server-chosen; it
/// becomes the verified audit actor (`service-token:CATALYRST_BADGES_ADMIN_TOKEN`).
const ADMIN_TOKEN_ENV: &str = "CATALYRST_BADGES_ADMIN_TOKEN";

/// A badges-local carrier for the configured admin secret, so the shared extractor's
/// `ConfiguredAdminBearerSecret: FromRef<S>` bound is satisfied by a *local* concrete state
/// type. Implementing `FromRef` for the foreign [`ConfiguredAdminBearerSecret`] over the
/// foreign `Arc<AppStateInner>` router state directly is forbidden by the orphan rule; this
/// local type is the legal seam.
#[derive(Clone)]
struct AdminSecretState(ConfiguredAdminBearerSecret);

impl FromRef<AdminSecretState> for ConfiguredAdminBearerSecret {
    fn from_ref(state: &AdminSecretState) -> Self {
        state.0.clone()
    }
}

/// Proof, wired into a handler's *signature*, that this request carried the badges admin
/// bearer secret.
///
/// The inner [`AuthenticatedAdminIdentity`] is a private tuple field: only this module can
/// mint a `RequireAdmin`, and only via [`establish_admin`], which runs the shared verified
/// extractor. A sibling module (e.g. the handlers) cannot construct one, and no external crate
/// can. It derives nothing -- no `Deserialize` (a request body must never become an admin
/// identity), no `Clone`/`Default` -- the same discipline as the shared type and
/// `catalyrst-server`'s `AdminSession`.
pub struct RequireAdmin(AuthenticatedAdminIdentity);

impl RequireAdmin {
    /// The server-verified audit actor, `service-token:CATALYRST_BADGES_ADMIN_TOKEN`. Built by
    /// the principal crate from the `&'static str` the operator configured -- it replaces the
    /// old client-supplied `x-catalyrst-admin` header value, which the server never verified.
    pub fn audit_actor_description(&self) -> String {
        self.0.audit_actor_description()
    }
}

/// Preserve the pre-migration badges wire contract: every admin-auth failure renders as a
/// **403** carrying the badges JSON error envelope, with the same messages the deleted
/// `check_admin` produced -- `"admin token not configured"` when the secret is unset,
/// `"missing or invalid bearer token"` for a missing or mismatched bearer. This deliberately
/// collapses the shared extractor's 503-vs-401 distinction back to 403; adopting 401/503 is a
/// separate follow-on.
fn to_api_error(rejection: AdminAuthRejection) -> ApiError {
    match rejection.refusal() {
        AuthorityNotEstablished::CredentialNotConfigured { .. } => {
            ApiError::forbidden("admin token not configured")
        }
        _ => ApiError::forbidden("missing or invalid bearer token"),
    }
}

/// The single mint for [`RequireAdmin`]: build the local secret carrier from the configured
/// token, run the shared verified extractor over the request parts, and map its rejection onto
/// the badges wire contract. Split out from the trait impl only so it is unit-testable without
/// a full `AppState` (which would require a live database).
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
            "service-token:CATALYRST_BADGES_ADMIN_TOKEN"
        );
    }

    // The pre-migration wire contract: 403 + the badges error envelope, same messages, for
    // every failure mode. These lock the behaviour the deleted `check_admin` had.
    #[tokio::test]
    async fn an_unset_token_fails_closed_as_403_not_configured() {
        for presented in [Some("Bearer anything"), None] {
            let (status, body) = reject_status_and_body(None, presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["ok"], serde_json::json!(false));
            assert_eq!(body["error"], "admin token not configured");
            assert_eq!(body["message"], "admin token not configured");
        }
    }

    #[tokio::test]
    async fn an_empty_configured_token_also_reads_as_not_configured() {
        let (status, body) = reject_status_and_body(Some(""), Some("Bearer ")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "admin token not configured");
    }

    #[tokio::test]
    async fn a_missing_or_wrong_bearer_is_403_invalid() {
        let cases = [None, Some("Bearer wrong"), Some("Basic s3cret")];
        for presented in cases {
            let (status, body) = reject_status_and_body(Some("s3cret"), presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["error"], "missing or invalid bearer token");
            assert_eq!(body["message"], "missing or invalid bearer token");
        }
    }
}
