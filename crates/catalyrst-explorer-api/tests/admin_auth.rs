//! Verifies every new admin route fails closed (403) without a valid bearer.
//!
//! These routes are gated by `CATALYRST_EXPLORER_API_ADMIN_TOKEN`. With the env
//! var unset the gate must 403 regardless of any (or no) Authorization header.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt; // for `oneshot`

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state};

async fn app() -> Router {
    // SAFETY-equivalent: ensure the token env is unset so the gate fails closed.
    std::env::remove_var("CATALYRST_EXPLORER_API_ADMIN_TOKEN");
    let cfg = Config::from_env().expect("config from env defaults");
    let state = build_state(&cfg).await.expect("build state");
    api_router().with_state(state)
}

async fn status_of(method: &str, path: &str, body: Body) -> StatusCode {
    let app = app().await;
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body)
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn admin_routes_unauthenticated_are_forbidden() {
    let cases: &[(&str, &str, &str)] = &[
        ("POST", "/admin/flags/toggle", r#"{"name":"x","value":true}"#),
        ("POST", "/admin/flags/reload", ""),
        ("POST", "/admin/blocklist/add", r#"{"wallet":"0xabc"}"#),
        ("POST", "/admin/blocklist/remove", r#"{"wallet":"0xabc"}"#),
        ("POST", "/admin/blocklist/reload", ""),
        ("GET", "/admin/config", ""),
        ("GET", "/admin/config/somekey", ""),
        ("PUT", "/admin/config/somekey", r#"{"value":1}"#),
        ("DELETE", "/admin/config/somekey", ""),
        ("GET", "/admin/auth/challenges", ""),
        ("GET", "/admin/auth/challenges/some-id", ""),
        ("POST", "/admin/auth/challenges/some-id/revoke", ""),
        ("GET", "/admin/auth/identities", ""),
        ("POST", "/admin/auth/identities/some-id/revoke", ""),
    ];

    for (method, path, body) in cases {
        let status = status_of(method, path, Body::from(body.to_string())).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {path} should be 403 without a bearer token, got {status}"
        );
    }
}
