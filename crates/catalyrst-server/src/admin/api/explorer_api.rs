use super::*;

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
