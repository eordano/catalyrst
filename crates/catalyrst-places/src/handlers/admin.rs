use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::RequireAdmin;
use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{PoiRow, ReportRow};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub status: Option<String>,
    pub entity_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/reports",
    tag = "admin",
    params(("limit" = Option<i64>, Query), ("offset" = Option<i64>, Query), ("status" = Option<String>, Query)),
    responses(
        (status = 200, body = ApiDataTotal<ReportRow>),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_reports(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(q): Query<ReportQuery>,
) -> Result<Json<ApiDataTotal<ReportRow>>, ApiError> {
    let status = q.status.as_deref().filter(|s| !s.is_empty());
    let entity_id = q.entity_id.as_deref().filter(|s| !s.is_empty());
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let total = state.places.count_reports(status, entity_id).await?;
    let rows = state
        .places
        .list_reports(status, entity_id, limit, offset)
        .await?;
    Ok(Json(ApiDataTotal::ok(rows, total)))
}

#[derive(Debug, Deserialize)]
pub struct ReportPatch {
    pub status: String,
    pub resolution: Option<String>,
    pub notes: Option<String>,

    pub resolved_by: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/reports/{id}",
    tag = "admin",
    params(("id" = i64, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = ApiData<ReportRow>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn patch_report(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(id): Path<i64>,
    Json(body): Json<ReportPatch>,
) -> Result<Json<ApiData<ReportRow>>, ApiError> {
    let status = body.status.trim();
    const ALLOWED: [&str; 4] = ["open", "resolved", "dismissed", "actioned"];
    if !ALLOWED.contains(&status) {
        return Err(ApiError::bad_request(
            "status must be one of: open, resolved, dismissed, actioned",
        ));
    }
    let row = state
        .places
        .update_report_status(
            id,
            status,
            body.resolution.as_deref(),
            body.notes.as_deref(),
            body.resolved_by.as_deref(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Report {} not found", id)))?;
    Ok(Json(ApiData::ok(row)))
}

#[derive(Debug, Deserialize)]
pub struct DisablePlace {
    pub disabled: Option<bool>,
    pub reason: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/places/{place_id}/disable",
    tag = "admin",
    params(("place_id" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn patch_place_disable(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(place_id): Path<String>,
    body: Option<Json<DisablePlace>>,
) -> Result<Json<Value>, ApiError> {
    let (disabled, reason) = match body {
        Some(Json(b)) => (b.disabled.unwrap_or(true), b.reason),
        None => (true, None),
    };
    let found = state
        .places
        .set_disabled(&place_id, disabled, reason.as_deref())
        .await?;
    if !found {
        return Err(ApiError::not_found(format!(
            "Not found place \"{}\"",
            place_id
        )));
    }
    Ok(Json(json!({
        "ok": true,
        "data": { "id": place_id, "disabled": disabled }
    })))
}

#[utoipa::path(
    get,
    path = "/pois",
    tag = "admin",
    responses(
        (status = 200, body = ApiData<Vec<PoiRow>>),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_pois(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> Result<Json<ApiData<Vec<PoiRow>>>, ApiError> {
    let rows = state.places.list_pois().await?;
    Ok(Json(ApiData::ok(rows)))
}

#[derive(Debug, Deserialize)]
pub struct PoiCreate {
    pub position: String,
    pub entity_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/pois",
    tag = "admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = ApiData<PoiRow>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn post_poi(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(body): Json<PoiCreate>,
) -> Result<Json<ApiData<PoiRow>>, ApiError> {
    let position = body.position.trim();
    if position.is_empty() {
        return Err(ApiError::bad_request("position is required"));
    }
    let row = state
        .places
        .upsert_poi(
            position,
            body.entity_id.as_deref(),
            body.title.as_deref(),
            body.description.as_deref(),
            body.enabled.unwrap_or(true),
            None,
        )
        .await?;
    Ok(Json(ApiData::ok(row)))
}

#[derive(Debug, Deserialize)]
pub struct PoiPatch {
    pub entity_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

#[utoipa::path(
    patch,
    path = "/pois/{position}",
    tag = "admin",
    params(("position" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = ApiData<PoiRow>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn patch_poi(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(position): Path<String>,
    Json(body): Json<PoiPatch>,
) -> Result<Json<ApiData<PoiRow>>, ApiError> {
    let row = state
        .places
        .update_poi(
            &position,
            body.entity_id.as_deref(),
            body.title.as_deref(),
            body.description.as_deref(),
            body.enabled,
        )
        .await?
        .ok_or_else(|| ApiError::not_found(format!("POI \"{}\" not found", position)))?;
    Ok(Json(ApiData::ok(row)))
}

#[utoipa::path(
    delete,
    path = "/pois/{position}",
    tag = "admin",
    params(("position" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_poi(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(position): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let removed = state.places.delete_poi(&position).await?;
    if !removed {
        return Err(ApiError::not_found(format!(
            "POI \"{}\" not found",
            position
        )));
    }
    Ok(Json(
        json!({ "ok": true, "data": { "position": position } }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::require_admin_bearer;
    use axum::http::HeaderMap;

    #[test]
    fn unauth_is_forbidden_with_token_set() {
        let headers = HeaderMap::new();
        let err = require_admin_bearer(&headers, Some("secret")).unwrap_err();
        assert!(matches!(
            err,
            ApiError::Common(catalyrst_types::ApiError::Http { status: 403, .. })
        ));
    }

    #[test]
    fn unset_token_is_forbidden() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer anything".parse().unwrap());
        let err = require_admin_bearer(&headers, None).unwrap_err();
        assert!(matches!(
            err,
            ApiError::Common(catalyrst_types::ApiError::Http { status: 403, .. })
        ));
    }

    #[test]
    fn wrong_token_is_forbidden() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer nope".parse().unwrap());
        let err = require_admin_bearer(&headers, Some("secret")).unwrap_err();
        assert!(matches!(
            err,
            ApiError::Common(catalyrst_types::ApiError::Http { status: 403, .. })
        ));
    }

    #[test]
    fn correct_token_passes() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&headers, Some("secret")).is_ok());
    }
}
