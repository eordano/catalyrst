use super::*;

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
