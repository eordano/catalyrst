use axum::http::HeaderValue;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

pub const DEFAULT_ORIGIN_SUFFIXES: &str = "decentraland.org,decentraland.zone,decentraland.today";

pub fn parse_origin_suffixes(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn origin_allowed(origin: &HeaderValue, suffixes: &[String]) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    suffixes.iter().any(|s| {
        host == *s
            || (host.len() > s.len()
                && host.ends_with(s.as_str())
                && host.as_bytes()[host.len() - s.len() - 1] == b'.')
    })
}

pub fn cors_layer(suffixes: Vec<String>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            origin_allowed(origin, &suffixes)
        }))
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
}

#[cfg(test)]
mod tests {
    use super::{cors_layer, origin_allowed, parse_origin_suffixes, DEFAULT_ORIGIN_SUFFIXES};
    use axum::body::Body;
    use axum::http::{HeaderValue, Request};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn allowed(origin: &str, raw_suffixes: &str) -> bool {
        origin_allowed(
            &HeaderValue::from_str(origin).unwrap(),
            &parse_origin_suffixes(raw_suffixes),
        )
    }

    #[test]
    fn suffix_match_anchors_on_label_boundaries_and_https() {
        assert!(allowed(
            "https://play.decentraland.org",
            DEFAULT_ORIGIN_SUFFIXES
        ));
        assert!(allowed(
            "https://decentraland.today",
            DEFAULT_ORIGIN_SUFFIXES
        ));
        assert!(allowed(
            "https://Play.Decentraland.ZONE",
            "decentraland.zone"
        ));
        assert!(!allowed(
            "https://evildecentraland.org",
            DEFAULT_ORIGIN_SUFFIXES
        ));
        assert!(!allowed(
            "https://decentraland.org.evil.com",
            DEFAULT_ORIGIN_SUFFIXES
        ));
        assert!(!allowed(
            "http://play.decentraland.org",
            DEFAULT_ORIGIN_SUFFIXES
        ));
        assert!(!allowed("https://example.com", DEFAULT_ORIGIN_SUFFIXES));
        assert!(!allowed("null", DEFAULT_ORIGIN_SUFFIXES));
    }

    #[test]
    fn parse_trims_dots_case_and_empties() {
        assert_eq!(
            parse_origin_suffixes(" .Decentraland.org , example.org ,,"),
            vec!["decentraland.org".to_string(), "example.org".to_string()]
        );
    }

    fn app() -> Router {
        Router::new()
            .route("/values/world", get(|| async { "ok" }))
            .layer(cors_layer(parse_origin_suffixes(DEFAULT_ORIGIN_SUFFIXES)))
    }

    fn preflight(origin: &str) -> Request<Body> {
        Request::builder()
            .method("OPTIONS")
            .uri("/values/world")
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn preflight_grants_allowed_origin() {
        let res = app()
            .oneshot(preflight("https://play.decentraland.org"))
            .await
            .unwrap();
        assert_eq!(
            res.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://play.decentraland.org")
        );
        assert!(res.headers().contains_key("access-control-allow-methods"));
    }

    #[tokio::test]
    async fn preflight_gives_no_grant_to_disallowed_origin() {
        let res = app()
            .oneshot(preflight("https://evil.example.com"))
            .await
            .unwrap();
        assert!(!res.headers().contains_key("access-control-allow-origin"));
    }

    #[tokio::test]
    async fn requests_without_origin_are_unaffected() {
        let res = app()
            .oneshot(
                Request::builder()
                    .uri("/values/world")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert!(!res.headers().contains_key("access-control-allow-origin"));
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
