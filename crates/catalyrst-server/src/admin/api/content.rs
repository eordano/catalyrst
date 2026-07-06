use super::*;

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
