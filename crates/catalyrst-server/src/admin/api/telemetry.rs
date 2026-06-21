use super::*;

pub async fn telemetry_issue_state(session: AdminSession, Json(body): Json<Value>) -> Response {
    let fingerprint = body
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    proxy_audited_global(
        &session.address,
        "telemetry.issue-state",
        fingerprint.as_deref(),
        body.clone(),
        Method::POST,
        "telemetry",
        "/dash/issue/state",
        Some(body),
        None,
    )
    .await
}

pub async fn telemetry_sql(session: AdminSession, Json(body): Json<Value>) -> Response {
    let Some(token) = env_token(TELEMETRY_TOKEN) else {
        return token_missing("telemetry");
    };
    proxy_audited_global(
        &session.address,
        "telemetry.sql",
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        "/dash/sql",
        Some(body),
        Some(&token),
    )
    .await
}

const TELEMETRY_TOKEN: &[&str] = &["CATALYRST_TELEMETRY_ADMIN_TOKEN"];

async fn telemetry_admin(
    state: &Arc<AppState>,
    addr: &str,
    action: &str,
    leaf: &str,
    body: Value,
) -> Response {
    let Some(token) = env_token(TELEMETRY_TOKEN) else {
        return token_missing("telemetry");
    };
    proxy_audited(
        state,
        addr,
        action,
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        &format!("/dash/admin/{leaf}"),
        Some(body),
        Some(&token),
    )
    .await
}

pub async fn telemetry_purge(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.purge", "purge", body).await
}

pub async fn telemetry_ingest(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.ingest", "ingest", body).await
}

pub async fn telemetry_quota(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.quota", "quota", body).await
}

pub async fn telemetry_bulk_delete(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.bulk-delete",
        "bulk-delete",
        body,
    )
    .await
}

pub async fn telemetry_export(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(&state, &session.address, "telemetry.export", "export", body).await
}

pub async fn telemetry_audit(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let Some(token) = env_token(TELEMETRY_TOKEN) else {
        return token_missing("telemetry");
    };
    let qs = query_from_obj(&body, &["fingerprint", "action", "limit"]);
    proxy_audited(
        &state,
        &session.address,
        "telemetry.audit",
        None,
        body.clone(),
        Method::POST,
        "telemetry",
        &format!("/dash/admin/audit{qs}"),
        None,
        Some(&token),
    )
    .await
}

pub async fn telemetry_regroup(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.regroup",
        "regroup",
        body,
    )
    .await
}

pub async fn telemetry_release(
    session: AdminSession,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    telemetry_admin(
        &state,
        &session.address,
        "telemetry.release",
        "release",
        body,
    )
    .await
}
