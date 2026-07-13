use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::fed::apply as fed_apply;
use crate::fed::authority;
use crate::fed::messages::ProfileSettingsUpdate;
use crate::handlers::federation::{emit_gossip, is_federation_envelope, preflight};
use crate::http::response::ApiError;
use crate::AppState;

fn ok(data: Value) -> Json<Value> {
    Json(json!({ "ok": true, "data": data }))
}

fn require_auth(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    crate::auth_chain::require_signer(headers, method, path)
        .map(|s| s.to_lowercase())
        .map_err(|_| ApiError::unauthorized("Unauthorized"))
}

pub async fn list_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = require_auth(&headers, "get", "/api/profiles/settings")?;
    authority::require_moderator(&state.pool, &user).await?;
    let list = fed_apply::list_settings(&state.pool).await?;
    Ok(ok(json!(list)))
}

pub async fn get_auth_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = require_auth(&headers, "get", "/api/profiles/me/settings")?;
    let mut settings = fed_apply::load_settings(&state.pool, &user).await?;

    if let Some(obj) = settings.as_object_mut() {
        obj.insert("subscriptions".into(), json!([]));
    }
    Ok(ok(settings))
}

pub async fn update_my_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !is_federation_envelope(&body) {
        return Err(ApiError::bad_request("missing signed body"));
    }
    let (signed, signer) = preflight::<ProfileSettingsUpdate>(&state, &headers, body).await?;
    if !signed.message.target.eq_ignore_ascii_case(&signer) {
        return Err(ApiError::forbidden(
            "me/settings only edits the signer's own profile",
        ));
    }
    let (applied, settings) =
        fed_apply::apply_profile_settings(&state.pool, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(&state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(ok(settings))
}

pub async fn get_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = format!("/api/profiles/{}/settings", profile_id);
    let user = require_auth(&headers, "get", &path)?;
    authority::require_moderator(&state.pool, &user).await?;
    let settings = fed_apply::load_settings(&state.pool, &profile_id.to_lowercase()).await?;
    Ok(ok(settings))
}

pub async fn update_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !is_federation_envelope(&body) {
        return Err(ApiError::bad_request("missing signed body"));
    }
    let (signed, signer) = preflight::<ProfileSettingsUpdate>(&state, &headers, body).await?;
    if !signed.message.target.eq_ignore_ascii_case(&profile_id) {
        return Err(ApiError::bad_request(
            "target does not match path profile_id",
        ));
    }
    authority::require_moderator(&state.pool, &signer).await?;
    let (applied, settings) =
        fed_apply::apply_profile_settings(&state.pool, &signed, &signer, None).await?;
    if applied.fresh {
        emit_gossip(&state, &signed, &applied.signature_hash, &signer).await;
    }
    Ok(ok(settings))
}
