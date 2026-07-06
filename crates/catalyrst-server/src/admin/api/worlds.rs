use super::*;

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
