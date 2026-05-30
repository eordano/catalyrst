use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

const ALLOW_METHODS: &str = "GET,HEAD,POST,PUT,DELETE,PATCH,OPTIONS";
// Includes the signed-fetch identity headers so a browser explorer can perform
// authenticated writes (deploys / lambdas writes) cross-origin — without them
// the CORS preflight for a signed POST is rejected. `Range`/`If-None-Match`
// let browsers issue conditional and partial content reads.
const ALLOW_HEADERS: &str = "Cache-Control,Content-Type,Origin,Accept,User-Agent,X-Upload-Origin,Range,If-None-Match,If-Modified-Since,X-Identity-Timestamp,X-Identity-Metadata,X-Identity-Auth-Chain-0,X-Identity-Auth-Chain-1,X-Identity-Auth-Chain-2,X-Identity-Auth-Chain-3";
const MAX_AGE: &str = "86400";

fn append_vary_origin(resp: &mut Response) {
    let headers = resp.headers_mut();
    match headers.get(header::VARY) {
        Some(existing) => {
            let combined = format!("{}, Origin", existing.to_str().unwrap_or(""));
            if let Ok(v) = HeaderValue::from_str(&combined) {
                headers.insert(header::VARY, v);
            }
        }
        None => {
            headers.insert(header::VARY, HeaderValue::from_static("Origin"));
        }
    }
}

pub async fn cors_middleware(req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let is_preflight = req.method() == Method::OPTIONS;

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
            HeaderValue::from_static(ALLOW_HEADERS),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(MAX_AGE),
        );
        if let Some(origin) = origin {
            if let Ok(ov) = HeaderValue::from_str(&origin) {
                h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, ov);
                append_vary_origin(&mut resp);
            }
        }
        add_security_headers(&mut resp);
        return resp;
    }

    let mut resp = next.run(req).await;

    if let Some(origin) = origin {
        if let Ok(ov) = HeaderValue::from_str(&origin) {
            let h = resp.headers_mut();
            h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, ov);
            append_vary_origin(&mut resp);
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
    async fn origin_is_reflected_without_credentials_and_with_vary() {
        let resp = app()
            .oneshot(req(Method::GET, Some("https://play.decentraland.org")))
            .await
            .unwrap();
        let h = resp.headers();
        assert_eq!(
            h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://play.decentraland.org"
        );
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none());
        assert_eq!(h.get(header::VARY).unwrap(), "Origin");
    }

    #[tokio::test]
    async fn preflight_is_204_with_full_allowlist() {
        let resp = app()
            .oneshot(req(Method::OPTIONS, Some("https://play.decentraland.org")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let h = resp.headers();
        assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_METHODS).unwrap(), ALLOW_METHODS);
        assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), ALLOW_HEADERS);
        assert_eq!(h.get(header::ACCESS_CONTROL_MAX_AGE).unwrap(), MAX_AGE);
        assert_eq!(
            h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://play.decentraland.org"
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
    }
}
