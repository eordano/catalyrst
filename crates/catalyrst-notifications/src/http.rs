pub use catalyrst_types::ApiError;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::unauthorized("signed fetch required").into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "signed fetch required", "message": "signed fetch required" })
        );
    }

    #[tokio::test]
    async fn internal_detail_is_not_published() {
        let resp =
            ApiError::Internal("sendgrid request failed: bad key".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "internal error", "message": "internal error" })
        );
    }
}
