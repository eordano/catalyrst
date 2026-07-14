use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

// Advertise only verbs that route somewhere: PUT and PATCH have no handler, so
// a preflight promising them is a lie a browser would cache. DELETE stays -- it
// serves scene unpublish (/scenes/{coord}), which upstream's content-server has
// no equivalent of, so our list is theirs (GET,HEAD,POST,OPTIONS) plus DELETE.
const ALLOW_METHODS: &str = "GET,HEAD,POST,DELETE,OPTIONS";

// Fallback for preflights that carry no Access-Control-Request-Headers.
// When the request names its headers we REFLECT them instead (like the nginx
// _cors.inc this replaces at the transparent-front cutover): auth chains are
// open-ended -- X-Identity-Auth-Chain-N grows with delegation depth and
// smart-wallet (EIP-1654) links, so any enumerated list is a ceiling that
// breaks signed login for someone. Upstream verification reads the headers by
// prefix, unbounded; upstream's own CORS list omits X-Identity-* entirely,
// which we already deliberately diverge from (see conformance cors fixtures).
const ALLOW_HEADERS: &str = "Cache-Control,Content-Type,Origin,Accept,User-Agent,X-Upload-Origin,Range,If-None-Match,If-Modified-Since,X-Identity-Timestamp,X-Identity-Metadata,X-Identity-Auth-Chain-0,X-Identity-Auth-Chain-1,X-Identity-Auth-Chain-2,X-Identity-Auth-Chain-3";
// 10 minutes, not a day: a wrong preflight verdict cannot linger in a browser
// cache long enough to outlast a fix. Raise once the ADR-44 header reflection
// above has settled.
const MAX_AGE: &str = "600";

pub async fn cors_middleware(req: Request, next: Next) -> Response {
    // Allow-Origin is the static wildcard, not the caller's Origin echoed back, so
    // presence is all that matters -- a shared cache need not key on the Origin value.
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
        // Allow-Headers is reflected from Access-Control-Request-Headers, so a
        // shared cache must key the preflight on it. Allow-Origin is a static
        // wildcard now, so Origin is deliberately absent from Vary.
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
        // Without this a browser reads only the six CORS-safelisted response headers, hiding
        // ETag and Retry-After from JS. The wildcard is safe because this API authenticates by
        // signature, never cookies; it never sets Allow-Credentials, under which * is ignored.
        // Handlers that set their own list (file serving) keep it.
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
        // Allow-Origin no longer varies by caller, so the actual response carries no Vary.
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
        // PUT and PATCH have no route, so a preflight must not promise them; DELETE does.
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
        // Allow-Headers is reflected from the request, so a shared cache keys on it;
        // Allow-Origin is a static wildcard, so Origin stays out of Vary.
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
