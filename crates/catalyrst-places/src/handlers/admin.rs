use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::require_admin_bearer;
use crate::http::errors::ApiError;
use crate::http::response::{ApiData, ApiDataTotal};
use crate::ports::places::{PoiRow, ReportRow};
use crate::AppState;

fn gate(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    require_admin_bearer(headers, state.admin_auth_token.as_deref())
}

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub status: Option<String>,
    pub entity_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn get_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ReportQuery>,
) -> Result<Json<ApiDataTotal<ReportRow>>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn patch_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ReportPatch>,
) -> Result<Json<ApiData<ReportRow>>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn patch_place_disable(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(place_id): Path<String>,
    body: Option<Json<DisablePlace>>,
) -> Result<Json<Value>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn get_pois(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiData<Vec<PoiRow>>>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn post_poi(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PoiCreate>,
) -> Result<Json<ApiData<PoiRow>>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn patch_poi(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(position): Path<String>,
    Json(body): Json<PoiPatch>,
) -> Result<Json<ApiData<PoiRow>>, ApiError> {
    gate(&state, &headers)?;
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

pub async fn delete_poi(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(position): Path<String>,
) -> Result<Json<Value>, ApiError> {
    gate(&state, &headers)?;
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
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn unset_token_is_forbidden() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer anything".parse().unwrap());
        let err = require_admin_bearer(&headers, None).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn wrong_token_is_forbidden() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer nope".parse().unwrap());
        let err = require_admin_bearer(&headers, Some("secret")).unwrap_err();
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn correct_token_passes() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer secret".parse().unwrap());
        assert!(require_admin_bearer(&headers, Some("secret")).is_ok());
    }
}
