use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Version};
use axum::middleware::Next;
use axum::response::Response;

fn requests_close(headers: &HeaderMap) -> bool {
    headers.get_all(header::CONNECTION).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("close"))
        })
    })
}

pub(crate) async fn keep_alive_hint(request: Request, next: Next) -> Response {
    let persistent = request.version() == Version::HTTP_11 && !requests_close(request.headers());
    let mut response = next.run(request).await;
    if persistent
        && response.status() != StatusCode::SWITCHING_PROTOCOLS
        && !response.headers().contains_key(header::CONNECTION)
    {
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
        response
            .headers_mut()
            .entry("keep-alive")
            .or_insert(HeaderValue::from_static("timeout=70"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn hints_only_apply_to_persistent_http11() {
        for (version, connection, expected) in [
            (Version::HTTP_11, None, true),
            (Version::HTTP_11, Some("keep-alive"), true),
            (Version::HTTP_11, Some("keep-alive, CLOSE"), false),
            (Version::HTTP_10, None, false),
            (Version::HTTP_2, None, false),
        ] {
            let app = Router::new()
                .route("/", get(|| async { "ok" }))
                .layer(axum::middleware::from_fn(keep_alive_hint));
            let mut request = Request::builder().uri("/").version(version);
            if let Some(value) = connection {
                request = request.header(header::CONNECTION, value);
            }
            let response = app
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.headers().contains_key("keep-alive"), expected);
            assert_eq!(
                response.headers().contains_key(header::CONNECTION),
                expected
            );
            if expected {
                assert_eq!(response.headers()["keep-alive"], "timeout=70");
            }
        }
    }

    #[tokio::test]
    async fn explicit_close_and_upgrade_responses_are_preserved() {
        for (status, value) in [
            (StatusCode::OK, "close"),
            (StatusCode::SWITCHING_PROTOCOLS, "upgrade"),
        ] {
            let app = Router::new()
                .route(
                    "/",
                    get(move || async move { (status, [(header::CONNECTION, value)], "") }),
                )
                .layer(axum::middleware::from_fn(keep_alive_hint));
            let response = app
                .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.headers()[header::CONNECTION], value);
            assert!(!response.headers().contains_key("keep-alive"));
        }
    }
}
