use std::sync::Arc;

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};

use crate::admin::audit;
use crate::admin::auth::AdminSession;
use crate::handlers::console;
use crate::state::AppState;

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

#[derive(serde::Deserialize)]
pub struct EntityIdReq {
    #[serde(default, alias = "entityId")]
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct DenylistReq {
    #[serde(alias = "entity_id", alias = "entityId")]
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct ToggleReq {
    pub enabled: bool,
}

fn valid_content_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 512 && s.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

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

#[derive(serde::Deserialize)]
pub struct UserModReq {
    pub address: String,
    #[serde(flatten)]
    pub extra: Value,
}

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

pub async fn social_user_ban(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(
        &session.address,
        "social.user-ban",
        Method::POST,
        &req.address,
        "bans",
        Some(req.extra),
    )
    .await
}

pub async fn social_user_unban(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(
        &session.address,
        "social.user-unban",
        Method::DELETE,
        &req.address,
        "bans",
        None,
    )
    .await
}

pub async fn social_user_warning(session: AdminSession, Json(req): Json<UserModReq>) -> Response {
    comms_user_mod(
        &session.address,
        "social.user-warning",
        Method::POST,
        &req.address,
        "warnings",
        Some(req.extra),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct SceneReloadReq {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "sceneId", default)]
    pub scene_id: Option<String>,
}

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

const PLACES_TOKEN: &[&str] = &["PLACES_ADMIN_AUTH_TOKEN"];

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

fn token_missing(crate_key: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": format!("{crate_key}-admin-token-not-configured") })),
    )
        .into_response()
}

const EVENTS_TOKEN: &[&str] = &["CATALYRST_EVENTS_ADMIN_TOKEN"];

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

const WORLDS_TOKEN: &[&str] = &["CATALYRST_WORLDS_ADMIN_TOKEN"];

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

const AB_TOKEN: &[&str] = &["API_ADMIN_TOKEN", "AB_REGISTRY_ADMIN_TOKEN"];

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

const CAMERA_REEL_TOKEN: &[&str] = &["CATALYRST_CAMERA_REEL_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct ImageIdReq {
    pub image_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

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

const BUILDER_TOKEN: &[&str] = &["CATALYRST_BUILDER_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct CollectionItemReq {
    pub collection_id: String,
    pub item_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

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

const COMMUNITIES_TOKEN: &[&str] = &["API_ADMIN_TOKEN"];

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

const NOTIFICATIONS_TOKEN: &[&str] = &["CATALYRST_NOTIFICATIONS_ADMIN_TOKEN"];

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

const BADGES_TOKEN: &[&str] = &["CATALYRST_BADGES_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct BadgeGrantReq {
    pub address: String,
    pub badge_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

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

const SOCIAL_RPC_TOKEN: &[&str] = &["CATALYRST_SOCIAL_RPC_ADMIN_TOKEN"];

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

const SCENE_STATE_TOKEN: &[&str] = &["CATALYRST_SCENE_STATE_ADMIN_TOKEN", "DEBUGGING_SECRET"];

#[derive(serde::Deserialize)]
pub struct SceneReq {
    pub scene: String,
}

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

const CREDITS_TOKEN: &[&str] = &["CATALYRST_CREDITS_ADMIN_TOKEN"];

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

const PRICE_TOKEN: &[&str] = &["CATALYRST_PRICE_ADMIN_TOKEN"];

#[derive(serde::Deserialize)]
pub struct PriceOverrideReq {
    pub token: String,
    pub vs: String,
    #[serde(flatten)]
    pub extra: Value,
}

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

const RPC_TOKEN: &[&str] = &["CATALYRST_RPC_ADMIN_TOKEN"];

pub async fn rpc_config(session: AdminSession, State(state): State<Arc<AppState>>) -> Response {
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

const EXPLORER_API_TOKEN: &[&str] = &["CATALYRST_EXPLORER_API_ADMIN_TOKEN"];

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

const TELEMETRY_TOKEN: &[&str] = &["CATALYRST_TELEMETRY_ADMIN_TOKEN"];

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

pub async fn telemetry_purge(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.purge", "purge", body).await
}

pub async fn telemetry_ingest(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.ingest", "ingest", body).await
}

pub async fn telemetry_quota(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.quota", "quota", body).await
}

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

pub async fn telemetry_export(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.export", "export", body).await
}

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
