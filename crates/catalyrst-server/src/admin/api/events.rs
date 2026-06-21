use super::*;

const EVENTS_TOKEN: &[&str] = &["CATALYRST_EVENTS_ADMIN_TOKEN"];

pub async fn events_create(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(EVENTS_TOKEN) else {
        return token_missing("events");
    };
    proxy_audited(
        &state,
        &session.address,
        "events.create",
        None,
        body.clone(),
        Method::POST,
        "explore",
        "/api/events",
        Some(body),
        Some(&token),
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct EventIdBodyReq {
    pub event_id: String,
    #[serde(flatten)]
    pub extra: Value,
}

pub async fn events_moderate(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EventIdBodyReq>,
) -> Response {
    let Some(token) = env_token(EVENTS_TOKEN) else {
        return token_missing("events");
    };
    if !valid_path_segment(&req.event_id) {
        return bad_segment("event-id");
    }
    proxy_audited(
        &state,
        &session.address,
        "events.moderate",
        Some(&req.event_id),
        req.extra.clone(),
        Method::PATCH,
        "explore",
        &format!("/api/events/{}", req.event_id),
        Some(req.extra),
        Some(&token),
    )
    .await
}
