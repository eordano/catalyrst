pub use catalyrst_types::ApiError;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use serde_json::json;

    #[tokio::test]
    async fn error_envelope_wire_shape() {
        let resp = ApiError::bad_request("missing q parameter").into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            v,
            json!({ "ok": false, "error": "missing q parameter", "message": "missing q parameter" })
        );
    }
}
