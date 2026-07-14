use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::ApiErrorBody;
use serde::Serialize;

use crate::rest::auth_chain::{AuthChainError, SignedFetchRejection};
use crate::rest::http::ApiError;

pub const ADR44_MESSAGE: &str = "This endpoint requires a signed fetch request. See ADR-44.";

/// How much of the refused metadata the gate echoes back, matching upstream's own cut.
const METADATA_ECHO_MAX: usize = 64;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SignedFetchGateBody {
    pub error: String,
    pub message: String,
}

impl SignedFetchGateBody {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: ADR44_MESSAGE.to_string(),
        }
    }
}

#[derive(Debug)]
pub enum CommError {
    BadRequest(String),
    NotAuthorized(String),
    NotFound(String),
    Status(StatusCode, String),
    SignedFetchGate { status: StatusCode, error: String },
    Internal,
}

impl CommError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        CommError::BadRequest(msg.into())
    }
    pub fn not_authorized(msg: impl Into<String>) -> Self {
        CommError::NotAuthorized(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        CommError::NotFound(msg.into())
    }
    pub fn status(code: StatusCode, msg: impl Into<String>) -> Self {
        CommError::Status(code, msg.into())
    }
}

impl IntoResponse for CommError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            CommError::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                ApiErrorBody::labeled("Bad request", message),
            ),
            CommError::NotAuthorized(message) => (
                StatusCode::UNAUTHORIZED,
                ApiErrorBody::labeled("Not Authorized", message),
            ),
            CommError::NotFound(message) => (
                StatusCode::NOT_FOUND,
                ApiErrorBody::labeled("Not Found", message),
            ),
            CommError::Status(code, message) => (code, ApiErrorBody::new(message)),
            // 401 auth-gate refusals carry the unified envelope like every other 401;
            // the two-field ADR-44 body is reserved for the 400s, where it matches
            // upstream byte for byte.
            CommError::SignedFetchGate { status, error } if status == StatusCode::UNAUTHORIZED => {
                (status, ApiErrorBody::labeled(error, ADR44_MESSAGE))
            }
            CommError::SignedFetchGate { status, error } => {
                return (status, Json(SignedFetchGateBody::new(error))).into_response();
            }
            CommError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorBody::new("Internal Server Error"),
            ),
        };
        (status, Json(body)).into_response()
    }
}

fn echo_metadata(metadata: &str) -> String {
    if metadata.chars().count() <= METADATA_ECHO_MAX {
        return metadata.to_string();
    }
    let head: String = metadata.chars().take(METADATA_ECHO_MAX).collect();
    format!("{head}\u{2026}")
}

pub fn signed_fetch_gate_parts(e: SignedFetchRejection) -> (StatusCode, String) {
    use AuthChainError as E;
    let e = match e {
        // The metadata gate answers before verification and names what it read, truncated
        // (upstream #492).
        SignedFetchRejection::RefusedMetadata(metadata) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid metadata content: {}", echo_metadata(&metadata)),
            )
        }
        SignedFetchRejection::Chain(e) => e,
    };
    match e {
        // A request presenting no usable chain lacks credentials entirely: 401. A chain
        // that is present but structurally broken is a malformed request: 400 per ADR-44.
        E::InsufficientLinks => (StatusCode::UNAUTHORIZED, "Invalid Auth Chain".to_string()),
        E::MalformedChain { .. } => (StatusCode::BAD_REQUEST, "Invalid Auth Chain".to_string()),
        E::InvalidTimestamp(v) => (
            StatusCode::BAD_REQUEST,
            format!("Invalid chain timestamp: {v}"),
        ),
        E::SceneSignerRejected => (
            StatusCode::BAD_REQUEST,
            "Invalid metadata content".to_string(),
        ),
        E::InvalidSignature(d) => (StatusCode::UNAUTHORIZED, format!("Invalid signature: {d}")),
        E::Expired { .. } => (StatusCode::UNAUTHORIZED, "Expired signature".to_string()),
        E::MissingTimestamp
        | E::ForbiddenSigner
        | E::EipNotImplemented
        | E::AddressMismatch { .. }
        | E::CatalystUnavailable(_) => (StatusCode::UNAUTHORIZED, "Invalid signature".to_string()),
    }
}

pub fn signed_fetch_gate(e: SignedFetchRejection) -> CommError {
    let (status, error) = signed_fetch_gate_parts(e);
    CommError::SignedFetchGate { status, error }
}

pub fn signed_fetch_gate_json(e: SignedFetchRejection) -> (StatusCode, Json<serde_json::Value>) {
    let (status, error) = signed_fetch_gate_parts(e);
    if status == StatusCode::UNAUTHORIZED {
        return (
            status,
            Json(serde_json::json!(ApiErrorBody::labeled(
                error,
                ADR44_MESSAGE
            ))),
        );
    }
    (
        status,
        Json(serde_json::json!({ "error": error, "message": ADR44_MESSAGE })),
    )
}

impl From<SignedFetchRejection> for CommError {
    fn from(e: SignedFetchRejection) -> Self {
        signed_fetch_gate(e)
    }
}

impl From<AuthChainError> for CommError {
    fn from(e: AuthChainError) -> Self {
        signed_fetch_gate(e.into())
    }
}

/// A community membership authority that could not be established, rendered for the
/// read-path handlers.
///
/// Behaviour-preserving: a refusal keeps the 401 those handlers have always answered with
/// (`CommError::not_authorized`), and a backing-store failure keeps the 500 they answered
/// when `ApiError::Database` reached [`CommError::Internal`]. What changes is that the two
/// can no longer be confused for one another on the way here.
impl From<catalyrst_authenticated_principal::AuthorityNotEstablished> for CommError {
    fn from(e: catalyrst_authenticated_principal::AuthorityNotEstablished) -> Self {
        use catalyrst_authenticated_principal::AuthorityNotEstablished as NotEstablished;
        match e {
            NotEstablished::RefusedLacksAuthority { detail } => CommError::NotAuthorized(detail),
            NotEstablished::AuthenticationMissingOrInvalid { detail } => {
                CommError::NotAuthorized(detail.to_string())
            }
            NotEstablished::PresentedSharedSecretDidNotMatch => {
                CommError::NotAuthorized("presented shared secret did not match".to_string())
            }
            NotEstablished::UndeterminedStoreUnavailable {
                store,
                reason_for_operators,
            } => {
                tracing::error!(
                    %store,
                    reason = %reason_for_operators,
                    "community membership authority could not be determined"
                );
                CommError::Internal
            }
            NotEstablished::CredentialNotConfigured {
                environment_variable,
            } => {
                tracing::error!(%environment_variable, "credential not configured");
                CommError::Internal
            }
        }
    }
}

impl From<sqlx::Error> for CommError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "sqlx error");
        CommError::Internal
    }
}

impl From<ApiError> for CommError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Http(h) => match h.code {
                400 => CommError::BadRequest(h.message),
                401 => CommError::NotAuthorized(h.message),
                404 => CommError::NotFound(h.message),
                code => match StatusCode::from_u16(code) {
                    Ok(status) => CommError::Status(status, h.message),
                    Err(_) => {
                        tracing::error!(code, message = %h.message, "upstream error");
                        CommError::Internal
                    }
                },
            },
            ApiError::InvalidParameter(p) => CommError::BadRequest(p.to_string()),
            ApiError::Database(db) => {
                tracing::error!(error = %db, "sqlx error");
                CommError::Internal
            }
            ApiError::Internal(s) => {
                tracing::error!(message = %s, "internal error");
                CommError::Internal
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = CommError::not_found("community not found").into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "Not Found", "message": "community not found" })
        );
    }

    #[tokio::test]
    async fn status_envelope_wire_shape() {
        let resp = CommError::status(StatusCode::SERVICE_UNAVAILABLE, "friends unavailable")
            .into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "friends unavailable", "message": "friends unavailable" })
        );
    }

    #[tokio::test]
    async fn refused_metadata_wire_shape() {
        // Upstream #492: a 400 from the metadata gate, echoing back what the gate read.
        let raw = r#"{"signer":"decentraland-kernel-scene"}"#;
        let resp = signed_fetch_gate(SignedFetchRejection::RefusedMetadata(raw.to_string()))
            .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({
                "error": format!("Invalid metadata content: {raw}"),
                "message": ADR44_MESSAGE
            })
        );
    }

    #[test]
    fn a_long_metadata_echo_is_cut_where_upstream_cuts_it() {
        let raw = format!(r#"{{"signer":"{}"}}"#, "a".repeat(120));
        let (status, error) =
            signed_fetch_gate_parts(SignedFetchRejection::RefusedMetadata(raw.clone()));
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let echoed = error.strip_prefix("Invalid metadata content: ").unwrap();
        assert_eq!(echoed.chars().count(), METADATA_ECHO_MAX + 1);
        assert!(echoed.starts_with(&raw[..METADATA_ECHO_MAX]));
        assert!(echoed.ends_with('\u{2026}'));
    }
}
