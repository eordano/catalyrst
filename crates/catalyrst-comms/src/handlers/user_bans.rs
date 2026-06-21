use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::extract::{validate_body, SchemaValidate};
use crate::http::{conflict, not_found_labeled, ApiError};
use crate::moderator::{authorize_moderator, ModeratorMode};
use crate::ports::user_bans::{BanWriteError, CreateBan, CreateWarning, LiftError};
use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ModeratorQuery {
    pub moderator: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BanPlayerBody {
    pub reason: String,
    pub duration: Option<i64>,
    #[serde(rename = "customMessage")]
    pub custom_message: Option<String>,
}

impl SchemaValidate for BanPlayerBody {
    fn schema_validate(value: &serde_json::Value) -> Result<(), String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "must be an object".to_string())?;
        for key in obj.keys() {
            if !matches!(key.as_str(), "reason" | "duration" | "customMessage") {
                return Err(format!("additional property not allowed: {key}"));
            }
        }
        match obj.get("reason").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => {}
            _ => return Err("reason must be a non-empty string".to_string()),
        }
        if let Some(d) = obj.get("duration") {
            match d.as_f64() {
                Some(n) if n > 0.0 => {}
                _ => return Err("duration must be a number greater than 0".to_string()),
            }
        }
        if let Some(m) = obj.get("customMessage") {
            if !m.is_string() {
                return Err("customMessage must be a string".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct WarnPlayerBody {
    pub reason: String,
}

impl SchemaValidate for WarnPlayerBody {
    fn schema_validate(value: &serde_json::Value) -> Result<(), String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "must be an object".to_string())?;
        for key in obj.keys() {
            if key != "reason" {
                return Err(format!("additional property not allowed: {key}"));
            }
        }
        match obj.get("reason").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err("reason must be a non-empty string".to_string()),
        }
    }
}

pub async fn get_user_ban_status(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = state.user_bans.get_status(&address).await?;
    let data = serde_json::to_value(status).unwrap_or(serde_json::json!({ "isBanned": false }));
    Ok(Json(serde_json::json!({ "data": data })))
}

pub async fn post_user_ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(q): Query<ModeratorQuery>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let banned_by = authorize_moderator(
        &state,
        &headers,
        "post",
        &format!("/users/{address}/bans"),
        ModeratorMode::Write,
        q.moderator.as_deref(),
    )?;

    let content_type = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let body: BanPlayerBody = validate_body(content_type, &body_bytes)?;

    let ban = state
        .user_bans
        .create_ban(CreateBan {
            banned_address: address,
            banned_by,
            reason: body.reason,
            custom_message: body.custom_message,
            duration_ms: body.duration,
        })
        .await
        .map_err(|e| match e {
            BanWriteError::AlreadyBanned(addr) => {
                conflict(format!("Player is already banned: {addr}"))
            }
            BanWriteError::Db(e) => e,
        })?;

    if state.livekit_configured {
        let state2 = state.clone();
        let addr = ban.banned_address.clone();
        tokio::spawn(async move {
            if let Err(e) = state2
                .room_service()
                .remove_participant_from_all_rooms(&addr)
                .await
            {
                tracing::warn!(error = %e, addr = %addr, "failed to kick banned user from all rooms");
            }
        });
    }

    let data = serde_json::to_value(ban).unwrap_or(serde_json::Value::Null);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": data })),
    ))
}

pub async fn delete_user_ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(q): Query<ModeratorQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let lifted_by = authorize_moderator(
        &state,
        &headers,
        "delete",
        &format!("/users/{address}/bans"),
        ModeratorMode::Write,
        q.moderator.as_deref(),
    )?;

    state
        .user_bans
        .lift_ban(&address, &lifted_by)
        .await
        .map_err(|e| match e {
            LiftError::NotFound(addr) => {
                not_found_labeled(format!("No active ban found for player: {addr}"))
            }
            LiftError::Db(e) => e,
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_user_warnings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_moderator(
        &state,
        &headers,
        "get",
        &format!("/users/{address}/warnings"),
        ModeratorMode::Read,
        None,
    )?;

    let warnings = state.user_bans.get_warnings(&address).await?;
    let data = serde_json::to_value(warnings).unwrap_or(serde_json::Value::Array(vec![]));
    Ok(Json(serde_json::json!({ "data": data })))
}

pub async fn post_user_warning(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
    Query(q): Query<ModeratorQuery>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let warned_by = authorize_moderator(
        &state,
        &headers,
        "post",
        &format!("/users/{address}/warnings"),
        ModeratorMode::Write,
        q.moderator.as_deref(),
    )?;

    let content_type = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let body: WarnPlayerBody = validate_body(content_type, &body_bytes)?;

    let warning = state
        .user_bans
        .create_warning(CreateWarning {
            warned_address: address,
            warned_by,
            reason: body.reason,
        })
        .await?;

    let data = serde_json::to_value(warning).unwrap_or(serde_json::Value::Null);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": data })),
    ))
}

pub async fn list_all_bans(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_moderator(&state, &headers, "get", "/bans", ModeratorMode::Read, None)?;

    let bans = state.user_bans.get_active_bans().await?;
    let data = serde_json::to_value(bans).unwrap_or(serde_json::Value::Array(vec![]));
    Ok(Json(serde_json::json!({ "data": data })))
}
