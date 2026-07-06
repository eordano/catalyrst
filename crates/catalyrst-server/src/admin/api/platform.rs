use super::*;

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
