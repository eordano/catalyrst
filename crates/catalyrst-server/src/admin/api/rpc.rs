use super::*;

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
