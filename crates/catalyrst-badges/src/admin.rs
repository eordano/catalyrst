//! Compile-forced admin authentication for the badges mutation endpoints.
//!
//! [`RequireAdmin`] is a value a handler must *name in its signature*: axum refuses a
//! handler unless every argument is a valid extractor, and `RequireAdmin`'s only
//! constructor is the [`FromRequestParts`] impl below, which delegates to the shared,
//! verified [`AuthenticatedAdminIdentity`] mint. Unlike the `authorize_admin(&state,
//! &headers)?` call it replaced, the check cannot be deleted from a handler body.
//! `tests/admin_routes_are_gated.rs` pins that.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};
use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::http::errors::ApiError;
use crate::AppState;

/// Server-chosen, never client-supplied; becomes the verified audit actor
/// (`service-token:CATALYRST_BADGES_ADMIN_TOKEN`).
const ADMIN_TOKEN_ENV: &str = "CATALYRST_BADGES_ADMIN_TOKEN";

/// Local carrier satisfying the shared extractor's `ConfiguredAdminBearerSecret: FromRef<S>`
/// bound: implementing `FromRef` for the foreign [`ConfiguredAdminBearerSecret`] over the
/// foreign `Arc<AppStateInner>` router state is orphan-forbidden.
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
/// The private inner field means only this module can mint one, and only via
/// [`establish_admin`]. It deliberately derives nothing -- no `Deserialize` (a request body
/// must never become an admin identity), no `Clone`/`Default`.
pub struct RequireAdmin(AuthenticatedAdminIdentity);

impl RequireAdmin {
    /// The server-verified audit actor, `service-token:CATALYRST_BADGES_ADMIN_TOKEN`, built
    /// from the operator-configured env var -- not from any client-supplied header.
    pub fn audit_actor_description(&self) -> String {
        self.0.audit_actor_description()
    }
}

/// Preserves the pre-migration badges wire contract: *every* admin-auth failure renders as a
/// **403** carrying the `{ok:false,error,message}` envelope. This deliberately collapses the
/// shared extractor's 503-vs-401 distinction; adopting 401/503 is a separate follow-on.
fn to_api_error(rejection: AdminAuthRejection) -> ApiError {
    match rejection.refusal() {
        AuthorityNotEstablished::CredentialNotConfigured { .. } => {
            ApiError::forbidden("admin token not configured")
        }
        _ => ApiError::forbidden("missing or invalid bearer token"),
    }
}

/// The single mint for [`RequireAdmin`]. Split out from the trait impl only so it is
/// unit-testable without a full `AppState` (which would require a live database).
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
