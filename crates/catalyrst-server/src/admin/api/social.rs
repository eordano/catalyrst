use super::*;

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
