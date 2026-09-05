use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug)]
pub struct ApiError {
    pub code: u16,
    pub message: String,
    pub error_label: Option<String>,
    pub is_internal: bool,
}

impl ApiError {
    pub fn http(code: u16, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            error_label: None,
            is_internal: false,
        }
    }

    pub fn labeled(code: u16, label: impl Into<String>, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            error_label: Some(label.into()),
            is_internal: false,
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError::http(400, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiError::http(404, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError {
            code: 500,
            message: msg.into(),
            error_label: None,
            is_internal: true,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "sqlx error");
        ApiError {
            code: 500,
            message: "Internal Server Error".to_string(),
            error_label: None,
            is_internal: true,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = if self.is_internal {
            ErrorBody {
                error: "Internal Server Error".to_string(),
                message: None,
            }
        } else if let Some(label) = self.error_label {
            ErrorBody {
                error: label,
                message: Some(self.message),
            }
        } else {
            ErrorBody {
                error: self.message,
                message: None,
            }
        };
        (status, Json(body)).into_response()
    }
}

pub fn not_implemented(msg: impl Into<String>) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorBody {
            error: msg.into(),
            message: None,
        }),
    )
        .into_response()
}

pub fn auth_error(status: u16, msg: impl Into<String>) -> ApiError {
    ApiError::http(status, msg)
}

pub fn forbidden(msg: impl Into<String>) -> ApiError {
    ApiError::http(403, msg)
}

pub fn unauthorized(msg: impl Into<String>) -> ApiError {
    ApiError::http(401, msg)
}

pub fn conflict(msg: impl Into<String>) -> ApiError {
    ApiError::labeled(409, "Conflict", msg)
}

pub fn not_found_labeled(msg: impl Into<String>) -> ApiError {
    ApiError::labeled(404, "Not Found", msg)
}

pub fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError::http(404, msg)
}

pub fn service_unavailable(msg: impl Into<String>) -> ApiError {
    ApiError::http(503, msg)
}

pub fn encode_path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = conflict("scene already banned").into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "error": "Conflict", "message": "scene already banned" })
        );
    }

    #[test]
    fn encode_path_segment_neutralizes_url_metacharacters() {
        assert_eq!(encode_path_segment("myworld.dcl.eth"), "myworld.dcl.eth");
        assert_eq!(encode_path_segment("a/b"), "a%2Fb");
        assert_eq!(encode_path_segment("a?x=1"), "a%3Fx%3D1");
        assert_eq!(encode_path_segment("a#frag"), "a%23frag");
        assert_eq!(encode_path_segment("a%2F"), "a%252F");
        assert_eq!(
            encode_path_segment("evil/../permissions"),
            "evil%2F..%2Fpermissions"
        );
        assert_eq!(
            encode_path_segment("name with space"),
            "name%20with%20space"
        );
    }
}
