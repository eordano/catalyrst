//! Admin mutation controls (WO-2).
//!
//! Every handler in this module takes `AdminSession` as its first argument, so
//! the type system guarantees it cannot run without a valid, unexpired,
//! allowlisted admin session cookie (see [`crate::admin::auth`]). Unauthed calls
//! never reach the handler body — the extractor 403s first.
//!
//! Two kinds of control:
//!  - **Content-local** — operates on this process's own [`AppState`]
//!    (`flush_deployments_cache` clears the deployments cache).
//!  - **Proxy** — forwards a request to a sibling bundle resolved from
//!    `CATALYRST_SERVICE_URLS` over the shared 2s reqwest client, attaching the
//!    appropriate downstream credential and reporting the result as JSON so the
//!    UI can toast success/failure.
//!
//! Downstream auth per target (env, read lazily so the binary boots without it;
//! a missing token surfaces as a `403` from the handler or a `401` reported from
//! downstream rather than a panic):
//!  - telemetry `/dash/issue/state`, `/dash/sql` — loopback-trusted, no token.
//!  - create (ab-registry) `/registry`, `/flush-cache` — `AB_REGISTRY_ADMIN_TOKEN`.
//!  - social (comms) `/users/{address}/bans|warnings` — `COMMS_MODERATOR_TOKEN`.
//!  - scene-state `/debugging/reload` — `DEBUGGING_SECRET` injected into the body.
//!
//! The LATER content-core controls (failed-deployment retry/clear, denylist
//! add/remove/list, snapshot regen, challenge refresh, sync pause/resume/force,
//! runtime read-only toggle, accepting-users allowlist) are now implemented as
//! additional content-local handlers backed by the extended `state.rs` traits.
//! Each is fail-closed: a backend that lacks the capability surfaces the
//! trait's "unsupported" error as a 501, and every one records an `admin_audit`
//! row keyed by the authenticated `AdminSession` address.
//!
//! Signature-required proxies (places highlight/rating, AB denylist) remain out
//! of scope here (they need bearer parity in the owning crate first).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};

use crate::admin::audit;
use crate::admin::auth::AdminSession;
use crate::handlers::console;
use crate::state::AppState;

// ───────────────────────── shared proxy helper ─────────────────────────

/// Forward `method path` to the sibling bundle registered under `key` in
/// `CATALYRST_SERVICE_URLS` (optionally attaching a bearer token + JSON body) and
/// build the uniform `{ok,status,body}` envelope the UI renders, also returning
/// whether the downstream succeeded so callers (e.g. [`proxy_audited`]) can label
/// an audit row without re-parsing the response. `ok` mirrors the downstream 2xx
/// status (a downstream 401/4xx is reported, not masked); transport failures
/// become `ok:false,status:0`.
///
/// Returns `Err(envelope)` only for the not-configured case (callers map it to a
/// 502); every reachable outcome is `Ok((downstream_ok, envelope))`.
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
    // Forward the authenticated admin identity so the downstream sibling can
    // record the true actor in its own audit trail. Only set when the value is
    // a well-formed EVM address (it never originates from caller-supplied input
    // here, but guard anyway so a malformed header can never be emitted).
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
            // Prefer a parsed JSON body so the UI can render structure; fall back
            // to the raw text for non-JSON downstream responses.
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

/// Proxy a sibling-crate admin call **and** write an `admin_audit` row keyed by
/// the authenticated address. Used by every cross-service handler added for the
/// new sibling admin endpoints: it forwards `method path` to bundle `key`
/// (attaching `bearer` when present), then records the outcome — `"ok"` when the
/// downstream returned 2xx, `"error"` otherwise (a `not-configured` bundle is an
/// error too). Recording is best-effort (never blocks/fails the proxy).
///
/// The proxy response body carries `{ok, status, body}` (see [`proxy`]); we peek
/// at `ok` purely to label the audit row — the caller still gets the full
/// envelope back unchanged.
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

/// Like [`proxy_audited`] but records via the process-global audit pool
/// ([`audit::record_global`]) instead of `State`. Used by the first-tranche proxy
/// handlers whose signatures (no `State` extractor) the gate tests depend on.
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
    audit::record_global(addr, action, target, detail, if ok { "ok" } else { "error" }).await;
    resp
}

/// Read the first set env credential among `names` (treating empty/whitespace as
/// unset). Lets the console accept either its own name or the sibling service's
/// own env name for the same token (e.g. `COMMS_MODERATOR_TOKEN` or the comms
/// crate's own `MODERATOR_TOKEN`), so a single-host deploy needn't duplicate it.
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

/// Strict EVM-address check — the only caller-supplied value interpolated into a
/// proxied path, so it must not be able to inject `/`, `..`, or query strings.
fn valid_eth_address(s: &str) -> bool {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    matches!(s, Some(rest) if rest.len() == 40 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Accept a scene / world identifier for the scene-state reload body. Rejects
/// control chars / whitespace and over-long input; this is a body field (not a
/// path), but unvalidated input shouldn't flow to a privileged downstream.
fn valid_scene_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// Strict single-path-segment check for any caller-supplied value that is
/// interpolated into a proxied downstream PATH (ids, positions, world names,
/// wallets, tokens, …). Rejects `/`, `?`, `#`, `..`, control chars and
/// whitespace so a value can never inject a path segment / query string and
/// steer the privileged (token-bearing) call at another downstream route.
fn valid_path_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s != "."
        && s != ".."
        && !s.contains("..")
        && s.chars().all(|c| {
            !c.is_control()
                && !c.is_whitespace()
                && !matches!(c, '/' | '\\' | '?' | '#' | '%')
        })
}

/// Reject with a uniform `400 invalid-<what>` envelope. Small helper so the many
/// per-segment guards below stay one-liners.
fn bad_segment(what: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "ok": false, "error": format!("invalid-{what}") })),
    )
        .into_response()
}

// ───────────────────────── content-local ─────────────────────────

/// `POST /admin/api/content/flush-cache` — drop every entry in the local
/// deployments cache so the next read repopulates from the database.
pub async fn flush_deployments_cache(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    state.deployments_cache.clear();
    audit::record(
        state.audit_pool.as_ref(),
        &session.address,
        "content.flush-cache",
        None,
        json!({}),
        "ok",
    )
    .await;
    Json(json!({ "ok": true, "cleared": true }))
}

// ─────────────────── content-local mutations (LATER tranche) ───────────────────
//
// Each handler is gated by `AdminSession` (its first argument), operates on the
// local `AppState` traits extended in `state.rs`, and records an `admin_audit`
// row. Backends that lack a capability return the trait's "unsupported" error,
// which these handlers surface as `501 Not Implemented` (fail-closed, never a
// false success).

/// Map a trait `Result<T, E>` into the uniform JSON envelope + audit row.
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
            audit::record(state.audit_pool.as_ref(), addr, action, target, detail, "ok").await;
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

#[derive(serde::Deserialize)]
pub struct EntityIdReq {
    /// Accept either `entityId` or `id`.
    #[serde(default, alias = "entityId")]
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct DenylistReq {
    /// The console form posts `entity_id`; accept that (and camelCase) as aliases.
    #[serde(alias = "entity_id", alias = "entityId")]
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct ToggleReq {
    pub enabled: bool,
}

/// Loosely validate a content identifier (entity id / hash / pointer / denylist
/// id) before it flows into a DB write or denylist mutation. Rejects control
/// chars, whitespace, and absurd lengths; this is a body field, not a path.
fn valid_content_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && s.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// `POST /admin/api/content/failed-deployments/retry` — re-attempt one failed
/// deployment by entity id (`Deployer::retry_failed_deployment`).
pub async fn content_retry_failed(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EntityIdReq>,
) -> Response {
    if !valid_content_id(&req.id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "invalid-entity-id" })),
        )
            .into_response();
    }
    let outcome = state
        .deployer
        .retry_failed_deployment(&req.id)
        .await
        .map(Value::String)
        .map_err(|errs| errs.join("; "));
    finish(
        &state,
        &session.address,
        "content.failed-deployments.retry",
        Some(&req.id),
        json!({ "entityId": req.id }),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/failed-deployments/clear` — clear one failed
/// deployment (when `{id}` is given) or all of them. Backed by
/// `Database::clear_failed_deployment` / `clear_all_failed_deployments`.
pub async fn content_clear_failed(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EntityIdReq>,
) -> Response {
    let (outcome, target, detail) = if req.id.is_empty() {
        (
            state
                .database
                .clear_all_failed_deployments()
                .await
                .map(|n| json!({ "removed": n, "scope": "all" }))
                .map_err(|e| e.to_string()),
            None,
            json!({ "scope": "all" }),
        )
    } else {
        if !valid_content_id(&req.id) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "error": "invalid-entity-id" })),
            )
                .into_response();
        }
        (
            state
                .database
                .clear_failed_deployment(&req.id)
                .await
                .map(|n| json!({ "removed": n, "scope": "one" }))
                .map_err(|e| e.to_string()),
            Some(req.id.clone()),
            json!({ "entityId": req.id }),
        )
    };
    finish(
        &state,
        &session.address,
        "content.failed-deployments.clear",
        target.as_deref(),
        detail,
        outcome,
    )
    .await
}

/// `POST /admin/api/content/denylist/add` — add an id to the local denylist.
pub async fn content_denylist_add(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DenylistReq>,
) -> Response {
    if !valid_content_id(&req.id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "invalid-id" })),
        )
            .into_response();
    }
    let outcome = state
        .denylist
        .add(&req.id)
        .map(|added| json!({ "added": added }));
    finish(
        &state,
        &session.address,
        "content.denylist.add",
        Some(&req.id),
        json!({ "id": req.id }),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/denylist/remove` — remove an id from the denylist.
pub async fn content_denylist_remove(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DenylistReq>,
) -> Response {
    if !valid_content_id(&req.id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "invalid-id" })),
        )
            .into_response();
    }
    let outcome = state
        .denylist
        .remove(&req.id)
        .map(|removed| json!({ "removed": removed }));
    finish(
        &state,
        &session.address,
        "content.denylist.remove",
        Some(&req.id),
        json!({ "id": req.id }),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/denylist/list` — snapshot the denylist. Read-only,
/// but gated + audited because it lives on the mutation surface.
pub async fn content_denylist_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let ids = state.denylist.list();
    finish(
        &state,
        &session.address,
        "content.denylist.list",
        None,
        json!({ "count": ids.len() }),
        Ok(json!({ "ids": ids })),
    )
    .await
}

/// `POST /admin/api/content/snapshots/regenerate` — trigger snapshot regen
/// (`SnapshotGenerator::trigger_regeneration`).
pub async fn content_snapshots_regenerate(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let outcome = state
        .snapshot_generator
        .trigger_regeneration()
        .map(Value::String);
    finish(
        &state,
        &session.address,
        "content.snapshots.regenerate",
        None,
        json!({}),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/challenge/refresh` — rotate the challenge text
/// (`ChallengeSupervisor::refresh`).
pub async fn content_challenge_refresh(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let text = state.challenge_supervisor.refresh();
    finish(
        &state,
        &session.address,
        "content.challenge.refresh",
        None,
        json!({}),
        Ok(json!({ "challenge": text })),
    )
    .await
}

/// `POST /admin/api/content/sync/pause` — record pause intent
/// (`SynchronizationState::pause`).
pub async fn content_sync_pause(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let outcome = state
        .synchronization_state
        .pause()
        .map(|_| json!({ "control": "paused" }));
    finish(
        &state,
        &session.address,
        "content.sync.pause",
        None,
        json!({}),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/sync/resume` — record resume intent.
pub async fn content_sync_resume(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let outcome = state
        .synchronization_state
        .resume()
        .map(|_| json!({ "control": "run" }));
    finish(
        &state,
        &session.address,
        "content.sync.resume",
        None,
        json!({}),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/sync/force` — request an immediate sync pass.
pub async fn content_sync_force(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let outcome = state
        .synchronization_state
        .force()
        .map(|_| json!({ "forced": true }));
    finish(
        &state,
        &session.address,
        "content.sync.force",
        None,
        json!({}),
        outcome,
    )
    .await
}

/// `POST /admin/api/content/read-only` — toggle the runtime read-only flag
/// (`AppState::set_read_only`). Body: `{ "enabled": bool }` (true ⇒ read-only).
pub async fn content_read_only(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleReq>,
) -> Response {
    let now = state.set_read_only(req.enabled);
    finish(
        &state,
        &session.address,
        "content.read-only",
        None,
        json!({ "enabled": req.enabled }),
        Ok(json!({ "readOnly": now })),
    )
    .await
}

/// `POST /admin/api/content/accepting-users` — toggle the accepting-users
/// allowlist flag (`AcceptingUsers::set_accepting`). Body: `{ "enabled": bool }`.
pub async fn content_accepting_users(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleReq>,
) -> Response {
    let outcome = state
        .accepting_users
        .set_accepting(req.enabled)
        .map(|_| json!({ "acceptingUsers": state.accepting_users.is_accepting() }));
    finish(
        &state,
        &session.address,
        "content.accepting-users",
        None,
        json!({ "enabled": req.enabled }),
        outcome,
    )
    .await
}

// ───────────────────────── telemetry (loopback-trusted) ─────────────────────────

/// `POST /admin/api/telemetry/issue-state` → telemetry `POST /dash/issue/state`.
/// Body (`{fingerprint,status,assignee,note}`) is forwarded verbatim.
pub async fn telemetry_issue_state(session: AdminSession, Json(body): Json<Value>) -> Response {
    let fingerprint = body
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    proxy_audited_global(
        &session.address,
        "telemetry.issue-state",
        fingerprint.as_deref(),
        body.clone(),
        Method::POST,
        "telemetry",
        "/dash/issue/state",
        Some(body),
        None,
    )
    .await
}

/// `POST /admin/api/telemetry/sql` → telemetry `POST /dash/sql`. Read-only SQL is
/// enforced downstream; this surface is gated by `AdminSession` (vs bare loopback).
pub async fn telemetry_sql(session: AdminSession, Json(body): Json<Value>) -> Response {
    proxy_audited_global(
        &session.address,
        "telemetry.sql",
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        "/dash/sql",
        Some(body),
        None,
    )
    .await
}

// ───────────────────────── create / ab-registry ─────────────────────────

/// `POST /admin/api/create/registry-reingest` → ab-registry `POST /registry`,
/// forwarding `AB_REGISTRY_ADMIN_TOKEN` if configured (downstream 401s otherwise,
/// and that 401 is reported back).
pub async fn create_registry_reingest(session: AdminSession, Json(body): Json<Value>) -> Response {
    let token = env_token(&["AB_REGISTRY_ADMIN_TOKEN", "API_ADMIN_TOKEN"]);
    proxy_audited_global(
        &session.address,
        "create.registry-reingest",
        None,
        body.clone(),
        Method::POST,
        "create",
        "/registry",
        Some(body),
        token.as_deref(),
    )
    .await
}

/// `POST /admin/api/create/flush-ab-cache` → ab-registry `DELETE /flush-cache`
/// (console route is POST for CSRF-form simplicity; the downstream call is DELETE).
pub async fn create_flush_ab_cache(session: AdminSession) -> Response {
    let token = env_token(&["AB_REGISTRY_ADMIN_TOKEN", "API_ADMIN_TOKEN"]);
    proxy_audited_global(
        &session.address,
        "create.flush-ab-cache",
        None,
        json!({}),
        Method::DELETE,
        "create",
        "/flush-cache",
        None,
        token.as_deref(),
    )
    .await
}

// ───────────────────────── social / comms moderation ─────────────────────────

#[derive(serde::Deserialize)]
pub struct UserModReq {
    pub address: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// Map a bundled comms moderation call. Returns `403` when no moderator token is
/// configured (so the UI hides these controls and unconfigured calls fail closed).
async fn comms_user_mod(
    admin_addr: &str,
    action: &str,
    method: Method,
    address: &str,
    leaf: &str,
    body: Option<Value>,
) -> Response {
    let Some(token) = env_token(&["COMMS_MODERATOR_TOKEN", "MODERATOR_TOKEN"]) else {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "comms-moderator-token-not-configured" })),
        )
            .into_response();
    };
    // `address` is interpolated into the downstream path — reject anything that
    // isn't a bare 0x-address so it can't inject `/`, `..`, or a query string and
    // steer the privileged (token-bearing) call at another comms route.
    if !valid_eth_address(address.trim()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid-address" })),
        )
            .into_response();
    }
    let address = address.trim().to_lowercase();
    let path = format!("/users/{address}/{leaf}");
    proxy_audited_global(
        admin_addr,
        action,
        Some(&address),
        body.clone().unwrap_or_else(|| json!({})),
        method,
        "social",
        &path,
        body,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social/user-ban` → comms `POST /users/{address}/bans`.
/// Body: `{address, reason?, duration?}` (everything but `address` is forwarded).
pub async fn social_user_ban(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(&session.address, "social.user-ban", Method::POST, &req.address, "bans", Some(req.extra)).await
}

/// `POST /admin/api/social/user-unban` → comms `DELETE /users/{address}/bans`.
pub async fn social_user_unban(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(&session.address, "social.user-unban", Method::DELETE, &req.address, "bans", None).await
}

/// `POST /admin/api/social/user-warning` → comms `POST /users/{address}/warnings`.
pub async fn social_user_warning(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(&session.address, "social.user-warning", Method::POST, &req.address, "warnings", Some(req.extra)).await
}

// ───────────────────────── scene-state ─────────────────────────

#[derive(serde::Deserialize)]
pub struct SceneReloadReq {
    /// Accept either `sceneId` or `name` from the console form.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "sceneId", default)]
    pub scene_id: Option<String>,
}

/// `POST /admin/api/scene/reload` → scene-state `POST /debugging/reload`.
/// scene-state expects `{secret, name}` in the body (confirmed in
/// `crates/catalyrst-scene-state/src/handlers.rs::reload`), so this handler
/// injects `DEBUGGING_SECRET` into the forwarded body. Returns `403` when the
/// secret is unset.
pub async fn scene_reload(session: AdminSession, Json(req): Json<SceneReloadReq>) -> Response {
    let Some(secret) = env_token(&["DEBUGGING_SECRET"]) else {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "debugging-secret-not-configured" })),
        )
            .into_response();
    };
    let Some(name) = req
        .name
        .or(req.scene_id)
        .map(|n| n.trim().to_string())
        .filter(|n| valid_scene_name(n))
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing-or-invalid-scene-name" })),
        )
            .into_response();
    };
    let body = json!({ "secret": secret, "name": name });
    proxy_audited_global(
        &session.address,
        "scene.reload",
        Some(&name),
        json!({ "name": name }),
        Method::POST,
        "scene-state",
        "/debugging/reload",
        Some(body),
        None,
    )
    .await
}

// ═══════════════════════ sibling-crate admin proxies ═══════════════════════
//
// Every handler below is gated by `AdminSession` (its first argument), forwards
// the request to a sibling crate's own admin route via the bundle key registered
// in `CATALYRST_SERVICE_URLS`, attaches that crate's bearer token from the env
// (reusing the `env_token` fallback pattern), and records an `admin_audit` row
// via `proxy_audited`. Any caller-supplied value interpolated into a downstream
// PATH is validated with `valid_path_segment` / `valid_eth_address` first.
//
// Bundle-key mapping (the path is forwarded literally; the bundle merges each
// sibling router, so paths are globally unique within a bundle):
//   places / events / worlds          → "explore"
//   ab-registry / camera-reel / builder → "create"
//   communities / notifications / badges → "social"
//   social-rpc                         → "social-rpc"
//   scene-state                        → "scene-state"
//   credits / price / rpc             → "data"
//   explorer-api                       → "explorer-api"
//   telemetry                          → "telemetry"

/// Build an optional `?k=v&…` query string from a JSON object body, skipping
/// null/empty values. Only string/number/bool scalars are forwarded; each part
/// is percent-encoded. Used by the proxies whose sibling route reads query
/// params (the console forms POST a small JSON body of params).
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

/// Extract an owned audit-target string from a JSON body field (e.g. the
/// `address`/`method`/`network` a mutation acts on). Owned so it doesn't borrow
/// `body` across the `Some(body)` move into the proxy call.
fn target_field(body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Pull the body sub-object minus any keys that were consumed as path segments
/// or query params, so the forwarded body carries only the downstream's expected
/// fields. Returns `None` when nothing remains (so no JSON body is sent).
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

// ───────────────────────── places (bundle: explore) ─────────────────────────

const PLACES_TOKEN: &[&str] = &["PLACES_ADMIN_AUTH_TOKEN"];

/// `POST /admin/api/places/reports` → places `GET /api/reports`.
/// Forwards `status,entity_id,limit,offset` as query params.
pub async fn places_reports_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    let qs = query_from_obj(&body, &["status", "entity_id", "limit", "offset"]);
    proxy_audited(
        &state,
        &session.address,
        "places.reports.list",
        None,
        body.clone(),
        Method::GET,
        "explore",
        &format!("/api/reports{qs}"),
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct IdBodyReq {
    pub id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/places/report-resolve` → places `PATCH /api/reports/{id}`.
pub async fn places_report_resolve(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("report-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.report.resolve",
        Some(&req.id),
        req.extra.clone(),
        Method::PATCH,
        "explore",
        &format!("/api/reports/{}", req.id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct PlaceIdBodyReq {
    pub place_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/places/place-disable` → places `PATCH /api/places/{place_id}/disable`.
pub async fn places_place_disable(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PlaceIdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.place_id) {
        return bad_segment("place-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.place.disable",
        Some(&req.place_id),
        req.extra.clone(),
        Method::PATCH,
        "explore",
        &format!("/api/places/{}/disable", req.place_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/places/pois-list` → places `GET /api/pois`.
pub async fn places_pois_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    proxy_audited(
        &state,
        &session.address,
        "places.pois.list",
        None,
        json!({}),
        Method::GET,
        "explore",
        "/api/pois",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/places/poi-create` → places `POST /api/pois`.
/// Body forwarded verbatim (`position` required, validated as a segment defensively).
pub async fn places_poi_create(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    let pos = body
        .get("position")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if pos.trim().is_empty() {
        return bad_segment("position");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.poi.create",
        Some(&pos),
        body.clone(),
        Method::POST,
        "explore",
        "/api/pois",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct PositionBodyReq {
    pub position: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/places/poi-update` → places `PATCH /api/pois/{position}`.
pub async fn places_poi_update(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PositionBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.position) {
        return bad_segment("position");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.poi.update",
        Some(&req.position),
        req.extra.clone(),
        Method::PATCH,
        "explore",
        &format!("/api/pois/{}", req.position),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct PositionReq {
    pub position: String,
}

/// `POST /admin/api/places/poi-delete` → places `DELETE /api/pois/{position}`.
pub async fn places_poi_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PositionReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.position) {
        return bad_segment("position");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.poi.delete",
        Some(&req.position),
        json!({ "position": req.position }),
        Method::DELETE,
        "explore",
        &format!("/api/pois/{}", req.position),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/places/place-highlight` → places `PUT /api/places/{place_id}/highlight`.
/// (Bearer parity added in the places crate; we send the admin bearer.)
pub async fn places_place_highlight(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PlaceIdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.place_id) {
        return bad_segment("place-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.place.highlight",
        Some(&req.place_id),
        req.extra.clone(),
        Method::PUT,
        "explore",
        &format!("/api/places/{}/highlight", req.place_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/places/place-rating` → places `PUT /api/places/{place_id}/rating`.
pub async fn places_place_rating(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PlaceIdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.place_id) {
        return bad_segment("place-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.place.rating",
        Some(&req.place_id),
        req.extra.clone(),
        Method::PUT,
        "explore",
        &format!("/api/places/{}/rating", req.place_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct WorldIdBodyReq {
    pub world_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/places/world-highlight` → places `PUT /api/worlds/{world_id}/highlight`.
pub async fn places_world_highlight(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorldIdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.world_id) {
        return bad_segment("world-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.world.highlight",
        Some(&req.world_id),
        req.extra.clone(),
        Method::PUT,
        "explore",
        &format!("/api/worlds/{}/highlight", req.world_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/places/world-rating` → places `PUT /api/worlds/{world_id}/rating`.
pub async fn places_world_rating(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorldIdBodyReq>,
) -> Response {
    let Some(token) = env_token(PLACES_TOKEN) else {
        return places_token_missing();
    };
    if !valid_path_segment(&req.world_id) {
        return bad_segment("world-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "places.world.rating",
        Some(&req.world_id),
        req.extra.clone(),
        Method::PUT,
        "explore",
        &format!("/api/worlds/{}/rating", req.world_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

fn places_token_missing() -> Response {
    token_missing("places")
}

/// Uniform fail-closed 403 when a crate's admin token env is unset. Mirrors the
/// comms/scene-state token-missing responses already in this module.
fn token_missing(crate_key: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": format!("{crate_key}-admin-token-not-configured") })),
    )
        .into_response()
}

// ───────────────────────── events (bundle: explore) ─────────────────────────

const EVENTS_TOKEN: &[&str] = &["CATALYRST_EVENTS_ADMIN_TOKEN"];

/// `POST /admin/api/events/create` → events `POST /api/events`. Body forwarded verbatim.
pub async fn events_create(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(EVENTS_TOKEN) else {
        return token_missing("events");
    };
    proxy_audited(
        &state,
        &session.address,
        "events.create",
        None,
        body.clone(),
        Method::POST,
        "explore",
        "/api/events",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct EventIdBodyReq {
    pub event_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/events/moderate` → events `PATCH /api/events/{event_id}`.
pub async fn events_moderate(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EventIdBodyReq>,
) -> Response {
    let Some(token) = env_token(EVENTS_TOKEN) else {
        return token_missing("events");
    };
    if !valid_path_segment(&req.event_id) {
        return bad_segment("event-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "events.moderate",
        Some(&req.event_id),
        req.extra.clone(),
        Method::PATCH,
        "explore",
        &format!("/api/events/{}", req.event_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

// ───────────────────────── worlds (bundle: explore) ─────────────────────────

const WORLDS_TOKEN: &[&str] = &["CATALYRST_WORLDS_ADMIN_TOKEN"];

/// `POST /admin/api/worlds/list` → worlds `GET /admin/worlds`. Forwards `limit,offset`.
pub async fn worlds_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    let qs = query_from_obj(&body, &["limit", "offset"]);
    proxy_audited(
        &state,
        &session.address,
        "worlds.list",
        None,
        body.clone(),
        Method::GET,
        "explore",
        &format!("/admin/worlds{qs}"),
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct WorldNameReq {
    pub world_name: String,
}

/// `POST /admin/api/worlds/detail` → worlds `GET /admin/worlds/{world_name}`.
pub async fn worlds_detail(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorldNameReq>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    if !valid_path_segment(&req.world_name) {
        return bad_segment("world-name");
    }
    proxy_audited(
        &state,
        &session.address,
        "worlds.detail",
        Some(&req.world_name),
        json!({ "world_name": req.world_name }),
        Method::GET,
        "explore",
        &format!("/admin/worlds/{}", req.world_name),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/enable` → worlds `POST /admin/worlds/{world_name}/enable`.
pub async fn worlds_enable(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorldNameReq>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    if !valid_path_segment(&req.world_name) {
        return bad_segment("world-name");
    }
    proxy_audited(
        &state,
        &session.address,
        "worlds.enable",
        Some(&req.world_name),
        json!({ "world_name": req.world_name }),
        Method::POST,
        "explore",
        &format!("/admin/worlds/{}/enable", req.world_name),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/disable` → worlds `POST /admin/worlds/{world_name}/disable`.
pub async fn worlds_disable(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorldNameReq>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    if !valid_path_segment(&req.world_name) {
        return bad_segment("world-name");
    }
    proxy_audited(
        &state,
        &session.address,
        "worlds.disable",
        Some(&req.world_name),
        json!({ "world_name": req.world_name }),
        Method::POST,
        "explore",
        &format!("/admin/worlds/{}/disable", req.world_name),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/ban-status` → worlds `GET /admin/worlds/{world_name}/ban-status`.
/// `world_name` is a path segment; `address` (required) + optional `parcel` ride
/// as query params on the proxied GET.
pub async fn worlds_ban_status(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    let world_name = body
        .get("world_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !valid_path_segment(world_name) {
        return bad_segment("world-name");
    }
    let qs = query_from_obj(&body, &["address", "parcel"]);
    proxy_audited(
        &state,
        &session.address,
        "worlds.ban-status",
        Some(world_name),
        body.clone(),
        Method::GET,
        "explore",
        &format!("/admin/worlds/{world_name}/ban-status{qs}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/blocked-list` → worlds `GET /admin/blocked`.
pub async fn worlds_blocked_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    proxy_audited(
        &state,
        &session.address,
        "worlds.blocked.list",
        None,
        json!({}),
        Method::GET,
        "explore",
        "/admin/blocked",
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct WalletReq {
    pub wallet: String,
}

/// `POST /admin/api/worlds/blocked-add` → worlds `POST /admin/blocked/{wallet}`.
pub async fn worlds_blocked_add(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WalletReq>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    if !valid_eth_address(req.wallet.trim()) {
        return bad_segment("wallet");
    }
    let wallet = req.wallet.trim().to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "worlds.blocked.add",
        Some(&wallet),
        json!({ "wallet": wallet }),
        Method::POST,
        "explore",
        &format!("/admin/blocked/{wallet}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/blocked-remove` → worlds `DELETE /admin/blocked/{wallet}`.
pub async fn worlds_blocked_remove(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WalletReq>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    if !valid_eth_address(req.wallet.trim()) {
        return bad_segment("wallet");
    }
    let wallet = req.wallet.trim().to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "worlds.blocked.remove",
        Some(&wallet),
        json!({ "wallet": wallet }),
        Method::DELETE,
        "explore",
        &format!("/admin/blocked/{wallet}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/worlds/access-log` → worlds `GET /admin/access-log`.
/// Forwards `world,address,limit,offset` query params.
pub async fn worlds_access_log(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(WORLDS_TOKEN) else {
        return token_missing("worlds");
    };
    let qs = query_from_obj(&body, &["world", "address", "limit", "offset"]);
    proxy_audited(
        &state,
        &session.address,
        "worlds.access-log",
        None,
        body.clone(),
        Method::GET,
        "explore",
        &format!("/admin/access-log{qs}"),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── ab-registry (bundle: create) ────────────────────
// Reuses API_ADMIN_TOKEN (already the create-bundle admin token), with the
// AB_REGISTRY_ADMIN_TOKEN fallback matching the existing create handlers.

const AB_TOKEN: &[&str] = &["API_ADMIN_TOKEN", "AB_REGISTRY_ADMIN_TOKEN"];

/// `POST /admin/api/create/queues-retry` → ab-registry `POST /queues/retry`.
pub async fn create_queues_retry(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    proxy_audited(
        &state,
        &session.address,
        "create.queues.retry",
        None,
        body.clone(),
        Method::POST,
        "create",
        "/queues/retry",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/create/queues-pause` → ab-registry `POST /queues/pause`.
pub async fn create_queues_pause(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    proxy_audited(
        &state,
        &session.address,
        "create.queues.pause",
        None,
        json!({}),
        Method::POST,
        "create",
        "/queues/pause",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/create/queues-resume` → ab-registry `POST /queues/resume`.
pub async fn create_queues_resume(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    proxy_audited(
        &state,
        &session.address,
        "create.queues.resume",
        None,
        json!({}),
        Method::POST,
        "create",
        "/queues/resume",
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct EntityIdPathReq {
    #[serde(alias = "entityId")]
    pub entity_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/create/denylist-add` → ab-registry `POST /denylist/{entity_id}`.
pub async fn create_denylist_add(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EntityIdPathReq>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    if !valid_path_segment(&req.entity_id) {
        return bad_segment("entity-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "create.denylist.add",
        Some(&req.entity_id),
        req.extra.clone(),
        Method::POST,
        "create",
        &format!("/denylist/{}", req.entity_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/create/denylist-remove` → ab-registry `DELETE /denylist/{entity_id}`.
pub async fn create_denylist_remove(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EntityIdPathReq>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    if !valid_path_segment(&req.entity_id) {
        return bad_segment("entity-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "create.denylist.remove",
        Some(&req.entity_id),
        json!({ "entityId": req.entity_id }),
        Method::DELETE,
        "create",
        &format!("/denylist/{}", req.entity_id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/create/queues-status` → ab-registry `GET /queues/status`.
/// Sends the admin bearer so the response includes the extra `paused` field.
pub async fn create_queues_status(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(AB_TOKEN) else {
        return token_missing("ab-registry");
    };
    proxy_audited(
        &state,
        &session.address,
        "create.queues.status",
        None,
        json!({}),
        Method::GET,
        "create",
        "/queues/status",
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── camera-reel (bundle: create) ────────────────────

const CAMERA_REEL_TOKEN: &[&str] = &["CATALYRST_CAMERA_REEL_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct ImageIdReq {
    pub image_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/camera-reel/image-delete` → camera-reel `DELETE /admin/images/{image_id}`.
pub async fn camera_reel_image_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImageIdReq>,
) -> Response {
    let Some(token) = env_token(CAMERA_REEL_TOKEN) else {
        return token_missing("camera-reel");
    };
    if !valid_path_segment(&req.image_id) {
        return bad_segment("image-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "camera-reel.image.delete",
        Some(&req.image_id),
        json!({ "image_id": req.image_id }),
        Method::DELETE,
        "create",
        &format!("/admin/images/{}", req.image_id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/camera-reel/image-review` → camera-reel `PATCH /admin/images/{image_id}/review`.
pub async fn camera_reel_image_review(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImageIdReq>,
) -> Response {
    let Some(token) = env_token(CAMERA_REEL_TOKEN) else {
        return token_missing("camera-reel");
    };
    if !valid_path_segment(&req.image_id) {
        return bad_segment("image-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "camera-reel.image.review",
        Some(&req.image_id),
        req.extra.clone(),
        Method::PATCH,
        "create",
        &format!("/admin/images/{}/review", req.image_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

// ──────────────────── builder (bundle: create) ────────────────────

const BUILDER_TOKEN: &[&str] = &["CATALYRST_BUILDER_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct CollectionItemReq {
    pub collection_id: String,
    pub item_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/builder/item-status` →
/// builder `PATCH /v1/collections/{id}/items/{item}/status`.
pub async fn builder_item_status(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CollectionItemReq>,
) -> Response {
    let Some(token) = env_token(BUILDER_TOKEN) else {
        return token_missing("builder");
    };
    if !valid_path_segment(&req.collection_id) {
        return bad_segment("collection-id");
    }
    if !valid_path_segment(&req.item_id) {
        return bad_segment("item-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "builder.item.status",
        Some(&req.item_id),
        req.extra.clone(),
        Method::PATCH,
        "create",
        &format!(
            "/v1/collections/{}/items/{}/status",
            req.collection_id, req.item_id
        ),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CollectionReq {
    pub collection_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/builder/items-status` →
/// builder `PATCH /v1/collections/{id}/items/status` (bulk).
pub async fn builder_items_status_bulk(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CollectionReq>,
) -> Response {
    let Some(token) = env_token(BUILDER_TOKEN) else {
        return token_missing("builder");
    };
    if !valid_path_segment(&req.collection_id) {
        return bad_segment("collection-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "builder.items.status",
        Some(&req.collection_id),
        req.extra.clone(),
        Method::PATCH,
        "create",
        &format!("/v1/collections/{}/items/status", req.collection_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

// ──────────────────── communities (bundle: social) ────────────────────
// Reuses API_ADMIN_TOKEN (the communities crate's own admin env).

const COMMUNITIES_TOKEN: &[&str] = &["API_ADMIN_TOKEN"];

/// `POST /admin/api/communities/list` → communities `GET /v1/admin/communities`.
/// Forwards `status,owner,search,limit,offset` query params.
pub async fn communities_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(COMMUNITIES_TOKEN) else {
        return token_missing("communities");
    };
    let qs = query_from_obj(&body, &["status", "owner", "search", "limit", "offset"]);
    proxy_audited(
        &state,
        &session.address,
        "communities.list",
        None,
        body.clone(),
        Method::GET,
        "social",
        &format!("/v1/admin/communities{qs}"),
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CommunityIdReq {
    pub id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/communities/suspend` →
/// communities `POST /v1/admin/communities/{id}/suspend`.
pub async fn communities_suspend(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CommunityIdReq>,
) -> Response {
    let Some(token) = env_token(COMMUNITIES_TOKEN) else {
        return token_missing("communities");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("community-id");
    }
    let body = body_without(&req.extra, &[]);
    proxy_audited(
        &state,
        &session.address,
        "communities.suspend",
        Some(&req.id),
        req.extra.clone(),
        Method::POST,
        "social",
        &format!("/v1/admin/communities/{}/suspend", req.id),
        body,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/communities/unsuspend` →
/// communities `POST /v1/admin/communities/{id}/unsuspend`.
pub async fn communities_unsuspend(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CommunityIdReq>,
) -> Response {
    let Some(token) = env_token(COMMUNITIES_TOKEN) else {
        return token_missing("communities");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("community-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "communities.unsuspend",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::POST,
        "social",
        &format!("/v1/admin/communities/{}/unsuspend", req.id),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── notifications (bundle: social) ────────────────────

const NOTIFICATIONS_TOKEN: &[&str] = &["CATALYRST_NOTIFICATIONS_ADMIN_TOKEN"];

/// `POST /admin/api/notifications/broadcast` → notifications `POST /notifications/broadcast`.
/// Body forwarded verbatim.
pub async fn notifications_broadcast(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(NOTIFICATIONS_TOKEN) else {
        return token_missing("notifications");
    };
    proxy_audited(
        &state,
        &session.address,
        "notifications.broadcast",
        None,
        body.clone(),
        Method::POST,
        "social",
        "/notifications/broadcast",
        Some(body),
        Some(&token),
    )
    .await
}

// ──────────────────── badges (bundle: social) ────────────────────

const BADGES_TOKEN: &[&str] = &["CATALYRST_BADGES_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct BadgeGrantReq {
    pub address: String,
    pub badge_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/badges/grant` → badges `POST /users/{address}/badges/{badge_id}`.
pub async fn badges_grant(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<BadgeGrantReq>,
) -> Response {
    let Some(token) = env_token(BADGES_TOKEN) else {
        return token_missing("badges");
    };
    if !valid_eth_address(req.address.trim()) {
        return bad_segment("address");
    }
    if !valid_path_segment(&req.badge_id) {
        return bad_segment("badge-id");
    }
    let address = req.address.trim().to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "badges.grant",
        Some(&req.badge_id),
        json!({ "address": address, "badge_id": req.badge_id, "body": req.extra }),
        Method::POST,
        "social",
        &format!("/users/{address}/badges/{}", req.badge_id),
        body_without(&req.extra, &[]),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct BadgeReq {
    pub address: String,
    pub badge_id: String,
}

/// `POST /admin/api/badges/revoke` → badges `DELETE /users/{address}/badges/{badge_id}`.
pub async fn badges_revoke(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<BadgeReq>,
) -> Response {
    let Some(token) = env_token(BADGES_TOKEN) else {
        return token_missing("badges");
    };
    if !valid_eth_address(req.address.trim()) {
        return bad_segment("address");
    }
    if !valid_path_segment(&req.badge_id) {
        return bad_segment("badge-id");
    }
    let address = req.address.trim().to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "badges.revoke",
        Some(&req.badge_id),
        json!({ "address": address, "badge_id": req.badge_id }),
        Method::DELETE,
        "social",
        &format!("/users/{address}/badges/{}", req.badge_id),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── social-rpc (bundle: social-rpc) ────────────────────

const SOCIAL_RPC_TOKEN: &[&str] = &["CATALYRST_SOCIAL_RPC_ADMIN_TOKEN"];

/// `POST /admin/api/social-rpc/presence` → social-rpc `GET /admin/social/presence`.
pub async fn social_rpc_presence(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.presence",
        None,
        json!({}),
        Method::GET,
        "social-rpc",
        "/admin/social/presence",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social-rpc/voice-calls` → social-rpc `GET /admin/social/voice-calls`.
/// Forwards `limit` query param.
pub async fn social_rpc_voice_calls(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    let qs = query_from_obj(&body, &["limit"]);
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.voice-calls",
        None,
        body.clone(),
        Method::GET,
        "social-rpc",
        &format!("/admin/social/voice-calls{qs}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social-rpc/friendships` → social-rpc `GET /admin/social/friendships/{address}`.
/// `address` is a path segment; `limit,offset` ride as query params.
pub async fn social_rpc_friendships(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    let address = body.get("address").and_then(|v| v.as_str()).unwrap_or("");
    if !valid_eth_address(address.trim()) {
        return bad_segment("address");
    }
    let address = address.trim().to_lowercase();
    let qs = query_from_obj(&body, &["limit", "offset"]);
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.friendships",
        Some(&address),
        body.clone(),
        Method::GET,
        "social-rpc",
        &format!("/admin/social/friendships/{address}{qs}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social-rpc/disconnect` → social-rpc `POST /admin/social/disconnect`.
pub async fn social_rpc_disconnect(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.disconnect",
        target_field(&body, "address").as_deref(),
        body.clone(),
        Method::POST,
        "social-rpc",
        "/admin/social/disconnect",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social-rpc/force-presence` → social-rpc `POST /admin/social/force-presence`.
pub async fn social_rpc_force_presence(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.force-presence",
        target_field(&body, "address").as_deref(),
        body.clone(),
        Method::POST,
        "social-rpc",
        "/admin/social/force-presence",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/social-rpc/reset-settings` → social-rpc `POST /admin/social/reset-settings`.
pub async fn social_rpc_reset_settings(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(SOCIAL_RPC_TOKEN) else {
        return token_missing("social-rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "social-rpc.reset-settings",
        target_field(&body, "address").as_deref(),
        body.clone(),
        Method::POST,
        "social-rpc",
        "/admin/social/reset-settings",
        Some(body),
        Some(&token),
    )
    .await
}

// ──────────────────── scene-state (bundle: scene-state) ────────────────────

const SCENE_STATE_TOKEN: &[&str] = &["CATALYRST_SCENE_STATE_ADMIN_TOKEN", "DEBUGGING_SECRET"];

#[derive(serde::Deserialize)]
pub struct SceneReq {
    pub scene: String,
}

/// `POST /admin/api/scene-state/crdt` → scene-state `GET /admin/scene/{scene}/crdt`.
pub async fn scene_state_crdt(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SceneReq>,
) -> Response {
    let Some(token) = env_token(SCENE_STATE_TOKEN) else {
        return token_missing("scene-state");
    };
    if !valid_path_segment(&req.scene) {
        return bad_segment("scene");
    }
    proxy_audited(
        &state,
        &session.address,
        "scene-state.crdt",
        Some(&req.scene),
        json!({ "scene": req.scene }),
        Method::GET,
        "scene-state",
        &format!("/admin/scene/{}/crdt", req.scene),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/scene-state/kick-all` → scene-state `POST /admin/scene/{scene}/kick-all`.
pub async fn scene_state_kick_all(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SceneReq>,
) -> Response {
    let Some(token) = env_token(SCENE_STATE_TOKEN) else {
        return token_missing("scene-state");
    };
    if !valid_path_segment(&req.scene) {
        return bad_segment("scene");
    }
    proxy_audited(
        &state,
        &session.address,
        "scene-state.kick-all",
        Some(&req.scene),
        json!({ "scene": req.scene }),
        Method::POST,
        "scene-state",
        &format!("/admin/scene/{}/kick-all", req.scene),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/scene-state/reset` → scene-state `POST /admin/scene/{scene}/reset`.
pub async fn scene_state_reset(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SceneReq>,
) -> Response {
    let Some(token) = env_token(SCENE_STATE_TOKEN) else {
        return token_missing("scene-state");
    };
    if !valid_path_segment(&req.scene) {
        return bad_segment("scene");
    }
    proxy_audited(
        &state,
        &session.address,
        "scene-state.reset",
        Some(&req.scene),
        json!({ "scene": req.scene }),
        Method::POST,
        "scene-state",
        &format!("/admin/scene/{}/reset", req.scene),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── credits (bundle: data) ────────────────────

const CREDITS_TOKEN: &[&str] = &["CATALYRST_CREDITS_ADMIN_TOKEN"];

/// `POST /admin/api/credits/seasons-list` → credits `GET /admin/seasons`.
pub async fn credits_seasons_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    proxy_audited(
        &state,
        &session.address,
        "credits.seasons.list",
        None,
        json!({}),
        Method::GET,
        "data",
        "/admin/seasons",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/season-create` → credits `POST /admin/seasons`.
pub async fn credits_season_create(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    proxy_audited(
        &state,
        &session.address,
        "credits.season.create",
        None,
        body.clone(),
        Method::POST,
        "data",
        "/admin/seasons",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CreditsIdReq {
    pub id: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/credits/season-update` → credits `PUT /admin/seasons/{id}`.
pub async fn credits_season_update(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreditsIdReq>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("season-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "credits.season.update",
        Some(&req.id),
        req.extra.clone(),
        Method::PUT,
        "data",
        &format!("/admin/seasons/{}", req.id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct IdOnlyReq {
    pub id: String,
}

/// `POST /admin/api/credits/season-delete` → credits `DELETE /admin/seasons/{id}`.
pub async fn credits_season_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdOnlyReq>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("season-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "credits.season.delete",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::DELETE,
        "data",
        &format!("/admin/seasons/{}", req.id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/goals-list` → credits `GET /admin/goals`. Forwards `weekId`.
pub async fn credits_goals_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    let qs = query_from_obj(&body, &["weekId"]);
    proxy_audited(
        &state,
        &session.address,
        "credits.goals.list",
        None,
        body.clone(),
        Method::GET,
        "data",
        &format!("/admin/goals{qs}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/goal-create` → credits `POST /admin/goals`.
pub async fn credits_goal_create(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    proxy_audited(
        &state,
        &session.address,
        "credits.goal.create",
        None,
        body.clone(),
        Method::POST,
        "data",
        "/admin/goals",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/goal-update` → credits `PUT /admin/goals/{id}`.
pub async fn credits_goal_update(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreditsIdReq>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("goal-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "credits.goal.update",
        Some(&req.id),
        req.extra.clone(),
        Method::PUT,
        "data",
        &format!("/admin/goals/{}", req.id),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/goal-delete` → credits `DELETE /admin/goals/{id}`.
pub async fn credits_goal_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdOnlyReq>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("goal-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "credits.goal.delete",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::DELETE,
        "data",
        &format!("/admin/goals/{}", req.id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/grant` → credits `POST /admin/credits/grant`.
pub async fn credits_grant(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    proxy_audited(
        &state,
        &session.address,
        "credits.grant",
        target_field(&body, "address").as_deref(),
        body.clone(),
        Method::POST,
        "data",
        "/admin/credits/grant",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/credits/revoke` → credits `POST /admin/credits/revoke`.
pub async fn credits_revoke(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    proxy_audited(
        &state,
        &session.address,
        "credits.revoke",
        target_field(&body, "address").as_deref(),
        body.clone(),
        Method::POST,
        "data",
        "/admin/credits/revoke",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CreditsBlockReq {
    pub address: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/credits/user-block` → credits `POST /admin/users/{address}/block`.
pub async fn credits_user_block(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreditsBlockReq>,
) -> Response {
    let Some(token) = env_token(CREDITS_TOKEN) else {
        return token_missing("credits");
    };
    if !valid_eth_address(req.address.trim()) {
        return bad_segment("address");
    }
    let address = req.address.trim().to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "credits.user.block",
        Some(&address),
        req.extra.clone(),
        Method::POST,
        "data",
        &format!("/admin/users/{address}/block"),
        Some(req.extra),
        Some(&token),
    )
    .await
}

// ──────────────────── price (bundle: data) ────────────────────

const PRICE_TOKEN: &[&str] = &["CATALYRST_PRICE_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct PriceOverrideReq {
    pub token: String,
    pub vs: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/price/override-set` →
/// price `PUT /admin/api/price/overrides/{token}/{vs}`.
pub async fn price_override_set(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PriceOverrideReq>,
) -> Response {
    let Some(token) = env_token(PRICE_TOKEN) else {
        return token_missing("price");
    };
    if !valid_path_segment(&req.token) {
        return bad_segment("token");
    }
    if !valid_path_segment(&req.vs) {
        return bad_segment("vs");
    }
    proxy_audited(
        &state,
        &session.address,
        "price.override.set",
        Some(&format!("{}/{}", req.token, req.vs)),
        req.extra.clone(),
        Method::PUT,
        "data",
        &format!("/admin/api/price/overrides/{}/{}", req.token, req.vs),
        Some(req.extra),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct PriceTokenVsReq {
    pub token: String,
    pub vs: String,
}

/// `POST /admin/api/price/override-delete` →
/// price `DELETE /admin/api/price/overrides/{token}/{vs}`.
pub async fn price_override_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PriceTokenVsReq>,
) -> Response {
    let Some(token) = env_token(PRICE_TOKEN) else {
        return token_missing("price");
    };
    if !valid_path_segment(&req.token) {
        return bad_segment("token");
    }
    if !valid_path_segment(&req.vs) {
        return bad_segment("vs");
    }
    proxy_audited(
        &state,
        &session.address,
        "price.override.delete",
        Some(&format!("{}/{}", req.token, req.vs)),
        json!({ "token": req.token, "vs": req.vs }),
        Method::DELETE,
        "data",
        &format!("/admin/api/price/overrides/{}/{}", req.token, req.vs),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── rpc (bundle: data) ────────────────────

const RPC_TOKEN: &[&str] = &["CATALYRST_RPC_ADMIN_TOKEN"];

/// `POST /admin/api/rpc/config` → rpc `GET /admin/rpc/config`.
pub async fn rpc_config(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.config",
        None,
        json!({}),
        Method::GET,
        "data",
        "/admin/rpc/config",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/methods-list` → rpc `GET /admin/rpc/methods`.
pub async fn rpc_methods_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.methods.list",
        None,
        json!({}),
        Method::GET,
        "data",
        "/admin/rpc/methods",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/methods-add` → rpc `POST /admin/rpc/methods`.
pub async fn rpc_methods_add(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.methods.add",
        target_field(&body, "method").as_deref(),
        body.clone(),
        Method::POST,
        "data",
        "/admin/rpc/methods",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/methods-remove` → rpc `DELETE /admin/rpc/methods`.
pub async fn rpc_methods_remove(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.methods.remove",
        target_field(&body, "method").as_deref(),
        body.clone(),
        Method::DELETE,
        "data",
        "/admin/rpc/methods",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/methods-reset` → rpc `POST /admin/rpc/methods/reset`.
pub async fn rpc_methods_reset(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.methods.reset",
        None,
        json!({}),
        Method::POST,
        "data",
        "/admin/rpc/methods/reset",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/networks-list` → rpc `GET /admin/rpc/networks`.
pub async fn rpc_networks_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.networks.list",
        None,
        json!({}),
        Method::GET,
        "data",
        "/admin/rpc/networks",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/rpc/networks-set` → rpc `POST /admin/rpc/networks`.
pub async fn rpc_networks_set(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    proxy_audited(
        &state,
        &session.address,
        "rpc.networks.set",
        target_field(&body, "network").as_deref(),
        body.clone(),
        Method::POST,
        "data",
        "/admin/rpc/networks",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct NetworkReq {
    pub network: String,
}

/// `POST /admin/api/rpc/networks-delete` → rpc `DELETE /admin/rpc/networks/{network}`.
pub async fn rpc_networks_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<NetworkReq>,
) -> Response {
    let Some(token) = env_token(RPC_TOKEN) else {
        return token_missing("rpc");
    };
    if !valid_path_segment(&req.network) {
        return bad_segment("network");
    }
    let network = req.network.to_lowercase();
    proxy_audited(
        &state,
        &session.address,
        "rpc.networks.delete",
        Some(&network),
        json!({ "network": network }),
        Method::DELETE,
        "data",
        &format!("/admin/rpc/networks/{network}"),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── explorer-api (bundle: explorer-api) ────────────────────

const EXPLORER_API_TOKEN: &[&str] = &["CATALYRST_EXPLORER_API_ADMIN_TOKEN"];

/// `POST /admin/api/explorer-api/flags-toggle` → explorer-api `POST /admin/flags/toggle`.
pub async fn explorer_api_flags_toggle(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.flags.toggle",
        target_field(&body, "name").as_deref(),
        body.clone(),
        Method::POST,
        "explorer-api",
        "/admin/flags/toggle",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/flags-reload` → explorer-api `POST /admin/flags/reload`.
pub async fn explorer_api_flags_reload(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.flags.reload",
        None,
        json!({}),
        Method::POST,
        "explorer-api",
        "/admin/flags/reload",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/blocklist-add` → explorer-api `POST /admin/blocklist/add`.
pub async fn explorer_api_blocklist_add(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.blocklist.add",
        target_field(&body, "wallet").as_deref(),
        body.clone(),
        Method::POST,
        "explorer-api",
        "/admin/blocklist/add",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/blocklist-remove` → explorer-api `POST /admin/blocklist/remove`.
pub async fn explorer_api_blocklist_remove(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.blocklist.remove",
        target_field(&body, "wallet").as_deref(),
        body.clone(),
        Method::POST,
        "explorer-api",
        "/admin/blocklist/remove",
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/blocklist-reload` → explorer-api `POST /admin/blocklist/reload`.
pub async fn explorer_api_blocklist_reload(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.blocklist.reload",
        None,
        json!({}),
        Method::POST,
        "explorer-api",
        "/admin/blocklist/reload",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/config-list` → explorer-api `GET /admin/config`.
pub async fn explorer_api_config_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.config.list",
        None,
        json!({}),
        Method::GET,
        "explorer-api",
        "/admin/config",
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct ConfigKeyReq {
    pub key: String,
    #[serde(flatten)]
    pub extra: Value,
}

/// `POST /admin/api/explorer-api/config-get` → explorer-api `GET /admin/config/{key}`.
pub async fn explorer_api_config_get(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigKeyReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.key) {
        return bad_segment("config-key");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.config.get",
        Some(&req.key),
        json!({ "key": req.key }),
        Method::GET,
        "explorer-api",
        &format!("/admin/config/{}", req.key),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/config-set` → explorer-api `PUT /admin/config/{key}`.
pub async fn explorer_api_config_set(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigKeyReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.key) {
        return bad_segment("config-key");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.config.set",
        Some(&req.key),
        req.extra.clone(),
        Method::PUT,
        "explorer-api",
        &format!("/admin/config/{}", req.key),
        Some(req.extra),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/config-delete` → explorer-api `DELETE /admin/config/{key}`.
pub async fn explorer_api_config_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeyOnlyReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.key) {
        return bad_segment("config-key");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.config.delete",
        Some(&req.key),
        json!({ "key": req.key }),
        Method::DELETE,
        "explorer-api",
        &format!("/admin/config/{}", req.key),
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct KeyOnlyReq {
    pub key: String,
}

/// `POST /admin/api/explorer-api/challenges-list` → explorer-api `GET /admin/auth/challenges`.
pub async fn explorer_api_challenges_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.challenges.list",
        None,
        json!({}),
        Method::GET,
        "explorer-api",
        "/admin/auth/challenges",
        None,
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct AuthIdReq {
    pub id: String,
}

/// `POST /admin/api/explorer-api/challenge-get` → explorer-api `GET /admin/auth/challenges/{id}`.
pub async fn explorer_api_challenge_get(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthIdReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("challenge-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.challenge.get",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::GET,
        "explorer-api",
        &format!("/admin/auth/challenges/{}", req.id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/challenge-revoke` →
/// explorer-api `POST /admin/auth/challenges/{id}/revoke`.
pub async fn explorer_api_challenge_revoke(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthIdReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("challenge-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.challenge.revoke",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::POST,
        "explorer-api",
        &format!("/admin/auth/challenges/{}/revoke", req.id),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/identities-list` → explorer-api `GET /admin/auth/identities`.
pub async fn explorer_api_identities_list(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.identities.list",
        None,
        json!({}),
        Method::GET,
        "explorer-api",
        "/admin/auth/identities",
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/explorer-api/identity-revoke` →
/// explorer-api `POST /admin/auth/identities/{id}/revoke`.
pub async fn explorer_api_identity_revoke(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthIdReq>,
) -> Response {
    let Some(token) = env_token(EXPLORER_API_TOKEN) else {
        return token_missing("explorer-api");
    };
    if !valid_path_segment(&req.id) {
        return bad_segment("identity-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "explorer-api.identity.revoke",
        Some(&req.id),
        json!({ "id": req.id }),
        Method::POST,
        "explorer-api",
        &format!("/admin/auth/identities/{}/revoke", req.id),
        None,
        Some(&token),
    )
    .await
}

// ──────────────────── telemetry (bundle: telemetry) ────────────────────

const TELEMETRY_TOKEN: &[&str] = &["CATALYRST_TELEMETRY_ADMIN_TOKEN"];

/// Generic telemetry admin proxy: `POST /dash/admin/{leaf}` with the bearer.
async fn telemetry_admin(
    state: &Arc<AppState>,
    addr: &str,
    action: &str,
    leaf: &str,
    body: Value,
) -> Response {
    let Some(token) = env_token(TELEMETRY_TOKEN) else {
        return token_missing("telemetry");
    };
    proxy_audited(
        state,
        addr,
        action,
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        &format!("/dash/admin/{leaf}"),
        Some(body),
        Some(&token),
    )
    .await
}

/// `POST /admin/api/telemetry/purge` → telemetry `POST /dash/admin/purge`.
pub async fn telemetry_purge(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.purge", "purge", body).await
}

/// `POST /admin/api/telemetry/ingest` → telemetry `POST /dash/admin/ingest`.
pub async fn telemetry_ingest(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.ingest", "ingest", body).await
}

/// `POST /admin/api/telemetry/quota` → telemetry `POST /dash/admin/quota`.
pub async fn telemetry_quota(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.quota", "quota", body).await
}

/// `POST /admin/api/telemetry/bulk-delete` → telemetry `POST /dash/admin/bulk-delete`.
pub async fn telemetry_bulk_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.bulk-delete",
        "bulk-delete",
        body,
    )
    .await
}

/// `POST /admin/api/telemetry/export` → telemetry `POST /dash/admin/export`.
pub async fn telemetry_export(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.export", "export", body).await
}

/// `POST /admin/api/telemetry/audit` → telemetry `POST /dash/admin/audit`.
/// The sibling reads `fingerprint,action,limit` as query params; forward them.
pub async fn telemetry_audit(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(TELEMETRY_TOKEN) else {
        return token_missing("telemetry");
    };
    let qs = query_from_obj(&body, &["fingerprint", "action", "limit"]);
    proxy_audited(
        &state,
        &session.address,
        "telemetry.audit",
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        &format!("/dash/admin/audit{qs}"),
        None,
        Some(&token),
    )
    .await
}

/// `POST /admin/api/telemetry/regroup` → telemetry `POST /dash/admin/regroup`.
pub async fn telemetry_regroup(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.regroup",
        "regroup",
        body,
    )
    .await
}

/// `POST /admin/api/telemetry/release` → telemetry `POST /dash/admin/release`.
pub async fn telemetry_release(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.release",
        "release",
        body,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    // Each mutation handler is gated by `AdminSession`. Without a valid session
    // cookie the extractor rejects the request with 403 before the body runs, so
    // we can assert the gate over the real router wiring without any downstream
    // bundle, token, or AppState configured.

    fn gated_router() -> Router {
        // Handlers that don't need State can be routed standalone; the extractor
        // runs identically regardless of State, which is what we're asserting.
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
        // No CATALYRST_SERVICE_URLS entry for this bogus key → 502 not-configured.
        let resp = proxy_audited_global(
            "0xtest", "test.unconfigured", None, serde_json::json!({}),
            Method::POST, "no-such-bundle", "/x", None, None,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
