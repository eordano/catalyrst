use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub code: u16,
    pub message: String,
    pub error_label: Option<String>,
    pub is_internal: bool,
    pub raw_body: Option<serde_json::Value>,
}

impl ApiError {
    pub fn http(code: u16, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            error_label: None,
            is_internal: false,
            raw_body: None,
        }
    }

    pub fn schema(code: u16, body: serde_json::Value) -> Self {
        ApiError {
            code,
            message: String::new(),
            error_label: None,
            is_internal: false,
            raw_body: Some(body),
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
            raw_body: None,
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
            raw_body: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if let Some(raw) = self.raw_body {
            return (status, Json(raw)).into_response();
        }
        let body = if self.is_internal {
            json!({ "error": "Internal Server Error" })
        } else if let Some(label) = self.error_label {
            json!({ "error": label, "message": self.message })
        } else {
            json!({ "error": self.message })
        };
        (status, Json(body)).into_response()
    }
}

pub fn not_implemented(msg: impl Into<String>) -> Response {
    let body = json!({ "error": msg.into() });
    (StatusCode::NOT_IMPLEMENTED, Json(body)).into_response()
}

pub fn auth_error(status: u16, msg: impl Into<String>) -> ApiError {
    ApiError::schema(status, json!({ "ok": false, "message": msg.into() }))
}

pub fn forbidden(msg: impl Into<String>) -> ApiError {
    ApiError::http(403, msg)
}

pub fn unauthorized(msg: impl Into<String>) -> ApiError {
    ApiError::http(401, msg)
}

pub fn conflict(msg: impl Into<String>) -> ApiError {
    ApiError {
        code: 409,
        message: msg.into(),
        error_label: Some("Conflict".to_string()),
        is_internal: false,
        raw_body: None,
    }
}

pub fn not_found_labeled(msg: impl Into<String>) -> ApiError {
    ApiError {
        code: 404,
        message: msg.into(),
        error_label: Some("Not Found".to_string()),
        is_internal: false,
        raw_body: None,
    }
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
    use super::encode_path_segment;

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
