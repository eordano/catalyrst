use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

const ALLOW_METHODS: &str = "GET,HEAD,POST,DELETE,OPTIONS";

const ALLOW_HEADERS: &str = "Cache-Control,Content-Type,Origin,Accept,User-Agent,X-Upload-Origin,Range,If-None-Match,If-Modified-Since,X-Identity-Timestamp,X-Identity-Metadata,X-Identity-Auth-Chain-0,X-Identity-Auth-Chain-1,X-Identity-Auth-Chain-2,X-Identity-Auth-Chain-3";
const MAX_AGE: &str = "600";

pub async fn cors_middleware(req: Request, next: Next) -> Response {
    let has_origin = req.headers().contains_key(header::ORIGIN);
    let is_preflight = req.method() == Method::OPTIONS;
    let requested_headers = req
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .cloned();

    if is_preflight {
        let mut resp = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap();
        let h = resp.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static(ALLOW_METHODS),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            requested_headers.unwrap_or(HeaderValue::from_static(ALLOW_HEADERS)),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(MAX_AGE),
        );
        h.insert(
            header::VARY,
            HeaderValue::from_static("Access-Control-Request-Headers"),
        );
        if has_origin {
            h.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            );
        }
        add_security_headers(&mut resp);
        return resp;
    }

    let mut resp = next.run(req).await;

    if has_origin {
        let h = resp.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        if !h.contains_key(header::ACCESS_CONTROL_EXPOSE_HEADERS) {
            h.insert(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("*"),
            );
        }
    }

    add_security_headers(&mut resp);
    resp
}

fn add_security_headers(resp: &mut Response) {
    resp.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(cors_middleware))
    }

    fn req(method: Method, origin: Option<&str>) -> Request {
        let mut b = Request::builder().method(method).uri("/x");
        if let Some(o) = origin {
            b = b.header(header::ORIGIN, o);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn no_origin_emits_no_cors_headers() {
        let resp = app().oneshot(req(Method::GET, None)).await.unwrap();
        let h = resp.headers();
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none());
        assert!(h.get(header::VARY).is_none());
        assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
    }

    #[tokio::test]
    async fn origin_is_wildcarded_without_credentials_or_vary() {
        let resp = app()
            .oneshot(req(Method::GET, Some("https://catalyst.example.com")))
            .await
            .unwrap();
        let h = resp.headers();
        assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none());
        assert_eq!(h.get(header::ACCESS_CONTROL_EXPOSE_HEADERS).unwrap(), "*");
        assert!(h.get(header::VARY).is_none());
    }

    #[tokio::test]
    async fn handler_chosen_expose_headers_survive() {
        let app = Router::new()
            .route(
                "/x",
                get(|| async { ([(header::ACCESS_CONTROL_EXPOSE_HEADERS, "ETag")], "ok") }),
            )
            .layer(axum::middleware::from_fn(cors_middleware));
        let resp = app
            .oneshot(req(Method::GET, Some("https://catalyst.example.com")))
            .await
            .unwrap();
        assert_eq!(
            resp.headers()
                .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
                .unwrap(),
            "ETag"
        );
    }

    #[tokio::test]
    async fn preflight_is_204_and_advertises_only_routed_methods() {
        let resp = app()
            .oneshot(req(Method::OPTIONS, Some("https://catalyst.example.com")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let h = resp.headers();
        let methods = h.get(header::ACCESS_CONTROL_ALLOW_METHODS).unwrap();
        assert_eq!(methods, "GET,HEAD,POST,DELETE,OPTIONS");
        assert!(!methods.to_str().unwrap().contains("PUT"));
        assert!(!methods.to_str().unwrap().contains("PATCH"));
        assert!(methods.to_str().unwrap().contains("DELETE"));
        assert_eq!(
            h.get(header::ACCESS_CONTROL_ALLOW_HEADERS).unwrap(),
            ALLOW_HEADERS
        );
        assert_eq!(h.get(header::ACCESS_CONTROL_MAX_AGE).unwrap(), "600");
        assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none());
        assert_eq!(
            h.get(header::VARY).unwrap(),
            "Access-Control-Request-Headers"
        );
    }

    #[tokio::test]
    async fn preflight_without_origin_still_204() {
        let resp = app().oneshot(req(Method::OPTIONS, None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(resp
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
        assert_eq!(
            resp.headers().get(header::VARY).unwrap(),
            "Access-Control-Request-Headers"
        );
    }
}
