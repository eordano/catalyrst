use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use catalyrst_authenticated_admin::{
    AdminAuthRejection, AuthenticatedAdminIdentity, ConfiguredAdminBearerSecret,
};
use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::http::response::ApiError;
use crate::AppState;

const ADMIN_TOKEN_ENV: &str = "CATALYRST_EVENTS_ADMIN_TOKEN";

#[derive(Clone)]
struct AdminSecretState(ConfiguredAdminBearerSecret);

impl FromRef<AdminSecretState> for ConfiguredAdminBearerSecret {
    fn from_ref(state: &AdminSecretState) -> Self {
        state.0.clone()
    }
}

pub struct RequireAdmin(AuthenticatedAdminIdentity);

impl RequireAdmin {
    pub fn audit_actor_description(&self) -> String {
        self.0.audit_actor_description()
    }
}

fn to_api_error(rejection: AdminAuthRejection) -> ApiError {
    match rejection.refusal() {
        AuthorityNotEstablished::CredentialNotConfigured { .. } => {
            ApiError::forbidden("Admin operations are disabled")
        }
        _ => ApiError::forbidden("You are not authorized to access this resource"),
    }
}

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
    async fn a_matching_bearer_establishes_admin() {
        let mut parts = parts_with_auth(Some("Bearer topsecret")).await;
        let admin = establish_admin(Some("topsecret".to_string()), &mut parts)
            .await
            .unwrap_or_else(|_| panic!("a matching secret establishes admin"));
        assert_eq!(
            admin.audit_actor_description(),
            "service-token:CATALYRST_EVENTS_ADMIN_TOKEN"
        );
    }

    #[tokio::test]
    async fn an_unset_token_fails_closed_as_403_disabled() {
        for presented in [Some("Bearer anything"), None] {
            let (status, body) = reject_status_and_body(None, presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["ok"], serde_json::json!(false));
            assert_eq!(body["error"], "Admin operations are disabled");
            assert_eq!(body["message"], "Admin operations are disabled");
        }
    }

    #[tokio::test]
    async fn a_missing_or_wrong_bearer_is_403_unauthorized() {
        let cases = [None, Some("Bearer wrong"), Some("Basic topsecret")];
        for presented in cases {
            let (status, body) = reject_status_and_body(Some("topsecret"), presented).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(
                body["error"],
                "You are not authorized to access this resource"
            );
            assert_eq!(
                body["message"],
                "You are not authorized to access this resource"
            );
        }
    }
}
