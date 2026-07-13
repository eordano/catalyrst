use super::*;

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
