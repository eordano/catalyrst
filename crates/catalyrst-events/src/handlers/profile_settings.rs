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

async fn require_auth(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<catalyrst_crypto::Signer, ApiError> {
    crate::auth_chain::require_signer(headers, method, path).await
}

#[utoipa::path(
    get,
    path = "/api/profiles/settings",
    tag = "profiles",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn list_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = require_auth(&headers, "get", "/api/profiles/settings").await?;
    authority::require_moderator(&state.pool, user.as_str()).await?;
    let list = fed_apply::list_settings(&state.pool).await?;
    Ok(ok(json!(list)))
}

#[utoipa::path(
    get,
    path = "/api/profiles/me/settings",
    tag = "profiles",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_auth_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let user = require_auth(&headers, "get", "/api/profiles/me/settings").await?;
    let mut settings = fed_apply::load_settings(&state.pool, user.as_str()).await?;

    if let Some(obj) = settings.as_object_mut() {
        obj.insert("subscriptions".into(), json!([]));
    }
    Ok(ok(settings))
}

#[utoipa::path(
    patch,
    path = "/api/profiles/me/settings",
    tag = "profiles",
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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

#[utoipa::path(
    get,
    path = "/api/profiles/{profile_id}/settings",
    tag = "profiles",
    params(("profile_id" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_profile_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = format!("/api/profiles/{}/settings", profile_id);
    let user = require_auth(&headers, "get", &path).await?;
    authority::require_moderator(&state.pool, user.as_str()).await?;
    let settings = fed_apply::load_settings(&state.pool, &profile_id.to_lowercase()).await?;
    Ok(ok(settings))
}

#[utoipa::path(
    patch,
    path = "/api/profiles/{profile_id}/settings",
    tag = "profiles",
    params(("profile_id" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
