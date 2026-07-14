use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub fn has_nul(s: &str) -> bool {
    s.contains('\0') || s.to_ascii_lowercase().contains("%00")
}

pub async fn nul_guard_middleware(req: Request, next: Next) -> Response {
    let uri = req.uri();
    if has_nul(uri.path()) || uri.query().map(has_nul).unwrap_or(false) {
        return crate::errors::bad_request("request path or query contains a NUL byte");
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(nul_guard_middleware))
    }

    async fn status(uri: &str) -> StatusCode {
        app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn rejects_nul_in_query() {
        assert_eq!(status("/x?deployedBy=%00").await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_nul_in_path() {
        assert_eq!(status("/x/%00").await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_uppercase_encoded_nul() {
        assert_eq!(status("/x?q=%00").await, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn allows_clean_request() {
        assert_eq!(status("/x?q=hello").await, StatusCode::OK);
    }

    #[test]
    fn has_nul_detects_encoded_and_raw() {
        assert!(has_nul("%00"));
        assert!(has_nul("%2500%00"));
        assert!(has_nul("emote/%00"));
        assert!(has_nul("a\0b"));
        assert!(!has_nul("abc"));
        assert!(!has_nul("0,0"));
    }
}
