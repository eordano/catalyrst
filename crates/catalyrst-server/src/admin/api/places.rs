use super::*;

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
