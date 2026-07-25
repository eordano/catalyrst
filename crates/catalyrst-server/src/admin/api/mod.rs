use std::sync::Arc;

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};

use crate::admin::audit;
use crate::admin::auth::AdminSession;
use crate::handlers::console;
use crate::state::AppState;

mod content;
mod create;
mod economy;
mod events;
mod explorer_api;
mod places;
mod platform;
mod rpc;
mod scene;
mod social;
mod telemetry;
mod worlds;

pub use content::*;
pub use create::*;
pub use economy::*;
pub use events::*;
pub use explorer_api::*;
pub use places::*;
pub use platform::*;
pub use rpc::*;
pub use scene::*;
pub use social::*;
pub use telemetry::*;
pub use worlds::*;

async fn proxy_envelope(
    method: Method,
    key: &str,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
    admin_addr: Option<&str>,
) -> Result<(bool, Value), Value> {
    let Some(base) = console::service_urls().get(key) else {
        return Err(json!({ "error": "not-configured", "service": key }));
    };

    let url = format!("{base}{path}");
    let mut req = console::client().request(method, &url);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }

    if let Some(addr) = admin_addr {
        if valid_eth_address(addr) {
            req = req.header("X-Catalyrst-Admin", addr);
        }
    }
    if let Some(b) = body {
        req = req.json(&b);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();

            let downstream: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => Value::String(text),
            };
            let ok = status.is_success();
            Ok((
                ok,
                json!({
                    "ok": ok,
                    "status": status.as_u16(),
                    "body": downstream,
                }),
            ))
        }
        Err(e) => Ok((
            false,
            json!({
                "ok": false,
                "status": 0,
                "body": { "error": "request-failed", "detail": e.to_string() },
            }),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn proxy_audited(
    state: &Arc<AppState>,
    addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    method: Method,
    key: &str,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> Response {
    let (ok, resp) = match proxy_envelope(method, key, path, body, bearer, Some(addr)).await {
        Ok((ok, env)) => (ok, Json(env).into_response()),
        Err(env) => (false, (StatusCode::BAD_GATEWAY, Json(env)).into_response()),
    };
    audit::record(
        state.audit_pool.as_ref(),
        addr,
        action,
        target,
        detail,
        if ok { "ok" } else { "error" },
    )
    .await;
    resp
}

#[allow(clippy::too_many_arguments)]
async fn proxy_audited_global(
    addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    method: Method,
    key: &str,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> Response {
    let (ok, resp) = match proxy_envelope(method, key, path, body, bearer, Some(addr)).await {
        Ok((ok, env)) => (ok, Json(env).into_response()),
        Err(env) => (false, (StatusCode::BAD_GATEWAY, Json(env)).into_response()),
    };
    audit::record_global(
        addr,
        action,
        target,
        detail,
        if ok { "ok" } else { "error" },
    )
    .await;
    resp
}

fn env_token(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(v) = std::env::var(name) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn valid_eth_address(s: &str) -> bool {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    matches!(s, Some(rest) if rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn valid_scene_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

fn valid_path_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s != "."
        && s != ".."
        && !s.contains("..")
        && s.chars().all(|c| {
            !c.is_control() && !c.is_whitespace() && !matches!(c, '/' | '\\' | '?' | '#' | '%')
        })
}

fn bad_segment(what: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "ok": false, "error": format!("invalid-{what}") })),
    )
        .into_response()
}

async fn finish(
    state: &Arc<AppState>,
    addr: &str,
    action: &str,
    target: Option<&str>,
    detail: Value,
    outcome: Result<Value, String>,
) -> Response {
    match outcome {
        Ok(body) => {
            audit::record(
                state.audit_pool.as_ref(),
                addr,
                action,
                target,
                detail,
                "ok",
            )
            .await;
            Json(json!({ "ok": true, "result": body })).into_response()
        }
        Err(e) => {
            audit::record(
                state.audit_pool.as_ref(),
                addr,
                action,
                target,
                json!({ "error": e }),
                "unsupported",
            )
            .await;
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "ok": false, "error": e })),
            )
                .into_response()
        }
    }
}

fn query_from_obj(body: &Value, keys: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(obj) = body.as_object() {
        for k in keys {
            match obj.get(*k) {
                Some(Value::String(s)) if !s.trim().is_empty() => {
                    parts.push(format!("{}={}", k, urlencoding::encode(s.trim())));
                }
                Some(Value::Number(n)) => parts.push(format!("{k}={n}")),
                Some(Value::Bool(b)) => parts.push(format!("{k}={b}")),
                _ => {}
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

fn target_field(body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn body_without(body: &Value, drop: &[&str]) -> Option<Value> {
    let obj = body.as_object()?;
    let filtered: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| !drop.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if filtered.is_empty() {
        None
    } else {
        Some(Value::Object(filtered))
    }
}

fn token_missing(crate_key: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": format!("{crate_key}-admin-token-not-configured") })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    fn gated_router() -> Router {
        Router::new()
            .route("/telemetry/sql", post(telemetry_sql))
            .route("/create/flush-ab-cache", post(create_flush_ab_cache))
            .route("/social/user-ban", post(social_user_ban))
            .route("/scene/reload", post(scene_reload))
    }

    async fn status_of(path: &str) -> StatusCode {
        let app = gated_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        resp.status()
    }

    #[tokio::test]
    async fn mutation_endpoints_403_without_cookie() {
        assert_eq!(status_of("/telemetry/sql").await, StatusCode::FORBIDDEN);
        assert_eq!(
            status_of("/create/flush-ab-cache").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(status_of("/social/user-ban").await, StatusCode::FORBIDDEN);
        assert_eq!(status_of("/scene/reload").await, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn proxy_unconfigured_service_is_502() {
        let resp = proxy_audited_global(
            "0xtest",
            "test.unconfigured",
            None,
            serde_json::json!({}),
            Method::POST,
            "no-such-bundle",
            "/x",
            None,
            None,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
