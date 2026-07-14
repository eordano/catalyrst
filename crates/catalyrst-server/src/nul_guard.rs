//! Reject requests whose URL (path or query) carries a NUL byte.
//!
//! PostgreSQL text values cannot contain `\0`. Without this guard a `%00` in a
//! path or query string decodes (via the `Path`/`parse_query_string`
//! extractors) into a real NUL, reaches a bound query parameter, and Postgres
//! rejects it — surfacing as an opaque `500 Internal Server Error` instead of a
//! clean `400`. This middleware rejects such requests at the edge, before any
//! handler or DB round-trip. The request-*body* counterpart (JSON `\0`) is
//! handled by `errors::AppError: From<DatabaseError>` and per-endpoint
//! validators (e.g. `handlers::active_entities`).
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// True if `s` carries a NUL byte, either raw (`\0`) or percent-encoded
/// (`%00`, any case). The raw request URI is still percent-encoded at
/// middleware time, so `%00` is what a URL-borne NUL actually looks like here.
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
        // The guard runs before routing, so a `%00` anywhere in the path is
        // rejected (400), not passed through to a 404.
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
        assert!(has_nul("%2500%00")); // trailing real-encoded NUL
        assert!(has_nul("emote/%00"));
        assert!(has_nul("a\0b"));
        assert!(!has_nul("abc"));
        assert!(!has_nul("0,0"));
    }
}
