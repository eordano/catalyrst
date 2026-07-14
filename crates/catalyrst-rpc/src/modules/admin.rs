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

fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = state
        .admin_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(StatusCode::FORBIDDEN)?;
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

pub const ALLOW_PRIVATE_UPSTREAM_ENV: &str = "CATALYRST_RPC_ALLOW_PRIVATE_UPSTREAM";

fn allow_private_upstream() -> bool {
    static ALLOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOW.get_or_init(|| {
        std::env::var(ALLOW_PRIVATE_UPSTREAM_ENV)
            .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
            .unwrap_or(false)
    })
}

fn is_blocked_upstream_host(url: &str) -> bool {
    is_blocked_upstream_host_with(url, allow_private_upstream())
}

fn is_blocked_upstream_host_with(url: &str, allow_private: bool) -> bool {
    if allow_private {
        return blocks_local_upstream_only(url);
    }
    !catalyrst_commons::http::is_safe_http_url(url)
}

/// The narrow deny-list a self-hosted deployment opts into: a compose service name
/// (`http://geth:8545`) or any other internal DNS name has to stay reachable, so only
/// the host's own stack, the reserved IP ranges, and the cloud metadata endpoint are
/// refused.
fn blocks_local_upstream_only(url: &str) -> bool {
    let after = url.split("://").nth(1).unwrap_or(url);
    let hostport = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = hostport.rsplit('@').next().unwrap_or(hostport);

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
        if let Some(v4) = ip.to_ipv4_mapped() {
            return v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified();
        }
        let seg = ip.segments();
        let is_ula = (seg[0] & 0xfe00) == 0xfc00;
        let is_ll = (seg[0] & 0xffc0) == 0xfe80;
        return ip.is_loopback() || ip.is_unspecified() || is_ula || is_ll;
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
    fn default_upstream_guard_refuses_internal_hosts() {
        for url in [
            "http://geth:8545",
            "http://erigon:8545",
            "http://eth.internal/rpc",
            "http://127.0.0.1:8545",
            "http://10.0.0.5:8545",
        ] {
            assert!(
                is_blocked_upstream_host_with(url, false),
                "{url} must be refused by default"
            );
        }
        assert!(!is_blocked_upstream_host_with(
            "https://rpc.example.com",
            false
        ));
    }

    #[test]
    fn opted_in_upstream_guard_allows_service_names_but_not_the_host_itself() {
        for url in ["http://geth:8545", "http://eth.internal/rpc"] {
            assert!(
                !is_blocked_upstream_host_with(url, true),
                "{url} must be reachable for a self-hosted deployment"
            );
        }
        for url in [
            "http://127.0.0.1:8545",
            "http://localhost:8545",
            "http://10.0.0.5:8545",
            "http://169.254.169.254/",
            "http://metadata.google.internal/",
        ] {
            assert!(
                is_blocked_upstream_host_with(url, true),
                "{url} must stay refused even with the opt-in"
            );
        }
        assert!(!is_blocked_upstream_host_with(
            "https://rpc.example.com",
            true
        ));
    }

    #[test]
    fn admin_auth_probe_compare_agrees_with_equality() {
        assert_eq!(
            timing_safe_eq("s3cr3t-token", "s3cr3t-token"),
            "s3cr3t-token" == "s3cr3t-token"
        );
        assert!(!timing_safe_eq("s3cr3t-token", "s3cr3t-tokeX"));
        assert!(!timing_safe_eq("s3cr3t-token", "X3cr3t-token"));
        assert!(!timing_safe_eq("s3cr3t-token", "s3cr3t-token-extra"));
        assert!(timing_safe_eq("", ""));
    }

    #[test]
    fn admin_auth_probe_unset_configured_token_rejects() {
        let st = state_with(None);
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer anything"))),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn admin_auth_probe_empty_configured_token_must_reject() {
        let st = state_with(Some(""));
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer "))),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer anything"))),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn admin_auth_probe_empty_presented_vs_set_secret_rejects() {
        let st = state_with(Some("the-real-token"));
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer "))),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_admin(&st, &hdr(Some("Bearer the-real-token"))),
            Ok(())
        );
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
