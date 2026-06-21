//! Bearer-gated runtime configuration for the relay.
//!
//! These routes let an operator view and amend the JSON-RPC method allowlist
//! (today seeded from [`crate::state::READ_ONLY_METHODS`]) and the network →
//! upstream map at runtime, without restarting the service. Both stores live
//! behind `RwLock`s on [`crate::state::AppStateInner`] and are read on the hot
//! relay path, so amendments take effect immediately.
//!
//! Every route is gated by a constant-time bearer-token compare against
//! `CATALYRST_RPC_ADMIN_TOKEN`. If that env is unset the routes fail closed
//! (403) — mirroring the timing-safe gate used in `catalyrst-comms`.

use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/rpc/config", get(get_config))
        .route(
            "/admin/rpc/methods",
            get(list_methods).post(add_method).delete(remove_method),
        )
        .route(
            "/admin/rpc/networks",
            get(list_networks).post(upsert_network),
        )
        .route(
            "/admin/rpc/networks/{network}",
            axum::routing::delete(remove_network),
        )
        .route("/admin/rpc/methods/reset", post(reset_methods))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time string compare (mirrors `catalyrst-comms::moderator::timing_safe_eq`).
fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Fail-closed bearer authorization. Returns `Ok(())` only when the admin token
/// env is configured *and* a matching `Authorization: Bearer` header is present.
fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = state.admin_token.as_deref().ok_or(StatusCode::FORBIDDEN)?;
    let token = bearer_token(headers).ok_or(StatusCode::FORBIDDEN)?;
    if timing_safe_eq(&token, expected) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

#[derive(Deserialize)]
struct MethodBody {
    method: String,
}

#[derive(Deserialize)]
struct NetworkBody {
    network: String,
    url: String,
}

// ----- combined config view -----

async fn get_config(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    (
        StatusCode::OK,
        Json(json!({
            "methods": state.methods_snapshot(),
            "networks": state.upstreams_snapshot(),
        })),
    )
        .into_response()
}

// ----- methods -----

async fn list_methods(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    (
        StatusCode::OK,
        Json(json!({ "methods": state.methods_snapshot() })),
    )
        .into_response()
}

async fn add_method(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MethodBody>,
) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    let method = body.method.trim().to_string();
    if method.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "method must be non-empty" })),
        )
            .into_response();
    }
    let added = {
        let mut set = state
            .allowed_methods
            .write()
            .expect("allowed_methods lock poisoned");
        set.insert(method.clone())
    };
    tracing::info!(%method, added, "admin amended method allowlist (add)");
    (
        StatusCode::OK,
        Json(json!({ "method": method, "added": added, "methods": state.methods_snapshot() })),
    )
        .into_response()
}

async fn remove_method(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MethodBody>,
) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    let method = body.method.trim().to_string();
    let removed = {
        let mut set = state
            .allowed_methods
            .write()
            .expect("allowed_methods lock poisoned");
        set.remove(&method)
    };
    tracing::info!(%method, removed, "admin amended method allowlist (remove)");
    (
        StatusCode::OK,
        Json(json!({ "method": method, "removed": removed, "methods": state.methods_snapshot() })),
    )
        .into_response()
}

async fn reset_methods(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    {
        let mut set = state
            .allowed_methods
            .write()
            .expect("allowed_methods lock poisoned");
        *set = crate::state::READ_ONLY_METHODS
            .iter()
            .map(|m| m.to_string())
            .collect();
    }
    tracing::info!("admin reset method allowlist to defaults");
    (
        StatusCode::OK,
        Json(json!({ "methods": state.methods_snapshot() })),
    )
        .into_response()
}

// ----- networks -----

async fn list_networks(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    (
        StatusCode::OK,
        Json(json!({ "networks": state.upstreams_snapshot() })),
    )
        .into_response()
}

/// True if `url`'s host is a loopback/private/link-local/metadata literal
/// that the RPC relay must not be pointed at (SSRF guard for the admin upstream
/// setter). Literal-host check only — DNS names are not resolved here.
fn is_blocked_upstream_host(url: &str) -> bool {
    let after = url.split("://").nth(1).unwrap_or(url);
    let hostport = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = hostport.rsplit('@').next().unwrap_or(hostport); // strip userinfo
                                                                // strip port (but keep IPv6 brackets intact)
    let host = if host.starts_with('[') {
        host.split(']')
            .next()
            .unwrap_or(host)
            .trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host == "metadata.google.internal"
    {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified();
    }
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        return ip.is_loopback() || ip.is_unspecified();
    }
    false
}

async fn upsert_network(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NetworkBody>,
) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    let network = body.network.trim().to_ascii_lowercase();
    let url = body.url.trim().to_string();
    if network.is_empty() || url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "network and url must be non-empty" })),
        )
            .into_response();
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "url must be http(s)" })),
        )
            .into_response();
    }
    // Defense-in-depth: this admin route otherwise turns the relay into an SSRF
    // primitive (set an internal upstream, then relay reflects its response).
    // Block loopback/private/link-local/metadata literal hosts. (Full
    // resolve-and-pin would also catch DNS names resolving internal; this admin
    // path is already gated, so the literal blocklist is the proportionate fix.)
    if is_blocked_upstream_host(&url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "upstream host not allowed" })),
        )
            .into_response();
    }
    {
        let mut map = state.upstreams.write().expect("upstreams lock poisoned");
        map.insert(network.clone(), url.clone());
    }
    tracing::info!(%network, %url, "admin upserted network upstream");
    (
        StatusCode::OK,
        Json(json!({ "network": network, "url": url, "networks": state.upstreams_snapshot() })),
    )
        .into_response()
}

async fn remove_network(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(network): Path<String>,
) -> impl IntoResponse {
    if let Err(s) = authorize_admin(&state, &headers) {
        return (s, Json(json!({ "error": "forbidden" }))).into_response();
    }
    let network = network.trim().to_ascii_lowercase();
    let removed = {
        let mut map = state.upstreams.write().expect("upstreams lock poisoned");
        map.remove(&network).is_some()
    };
    tracing::info!(%network, removed, "admin removed network upstream");
    (
        StatusCode::OK,
        Json(json!({ "network": network, "removed": removed, "networks": state.upstreams_snapshot() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::AppStateInner;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::sync::{Arc, RwLock};

    fn state_with(token: Option<&str>) -> AppState {
        let cfg = Config {
            http_host: "127.0.0.1".into(),
            http_port: 0,
            upstreams: HashMap::new(),
        };
        Arc::new(AppStateInner {
            cfg,
            http: reqwest::Client::new(),
            allowed_methods: RwLock::new(
                crate::state::READ_ONLY_METHODS
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<BTreeSet<String>>(),
            ),
            upstreams: RwLock::new(BTreeMap::new()),
            admin_token: token.map(|t| t.to_string()),
        })
    }

    fn hdr(value: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(v) = value {
            h.insert("authorization", v.parse().unwrap());
        }
        h
    }

    #[test]
    fn unset_token_fails_closed() {
        let st = state_with(None);
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer anything"))),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn missing_header_is_forbidden() {
        let st = state_with(Some("secret"));
        assert_eq!(authorize_admin(&st, &hdr(None)), Err(StatusCode::FORBIDDEN));
    }

    #[test]
    fn wrong_token_is_forbidden() {
        let st = state_with(Some("secret"));
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer nope"))),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn correct_token_authorizes() {
        let st = state_with(Some("secret"));
        assert_eq!(authorize_admin(&st, &hdr(Some("Bearer secret"))), Ok(()));
    }

    #[test]
    fn timing_safe_eq_basic() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "abcd"));
    }

    #[test]
    fn methods_mutation_round_trips() {
        let st = state_with(Some("t"));
        assert!(!st.is_method_allowed("eth_newFilter"));
        st.allowed_methods
            .write()
            .unwrap()
            .insert("eth_newFilter".into());
        assert!(st.is_method_allowed("eth_newFilter"));
    }
}
