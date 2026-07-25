use super::*;

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
