use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{parse_pagination, require_confirm_delete_all, UpsertEnvBody};
use crate::http::errors::ApiError;
use crate::storage::{check_limits, value_size_bytes};
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

/// GET /env/:key — authorized addresses only (the authoritative server).
pub async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::AUTHORIZED_ADDRESSES_ONLY).await?;

    let enc = state
        .storage
        .env_get_enc(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    match enc {
        // Upstream gates the 404 on JS truthiness (`if (!value)`) of the
        // decrypted string, so an empty-string env var reads back as not found.
        Some(blob) => {
            let value = state.encryptor.decrypt(&blob)?;
            if value.is_empty() {
                return Err(ApiError::not_found("Value not found"));
            }
            Ok(Json(json!({ "value": value })))
        }
        None => Err(ApiError::not_found("Value not found")),
    }
}

/// PUT /env/:key — owners and deployers only.
pub async fn upsert(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<UpsertEnvBody>,
) -> Result<StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    let size = value_size_bytes(&body.value);
    let info = state.storage.env_size_info(&ctx.world_name, Some(&key)).await?;
    check_limits(size, info, state.cfg.env_limits)?;

    let enc = state.encryptor.encrypt(&body.value)?;
    state
        .storage
        .env_set(&ctx.world_name, &ctx.place_id, &key, &enc, size)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /env/:key — owners and deployers only.
pub async fn delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    state
        .storage
        .env_delete(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /env — list key names only (never values); owners and deployers only.
pub async fn list_keys(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    let p = parse_pagination(&params)?;
    let keys = state
        .storage
        .env_list_keys(
            &ctx.world_name,
            &ctx.place_id,
            p.limit,
            p.offset,
            p.prefix.as_deref(),
        )
        .await?;
    let total = state
        .storage
        .env_count(&ctx.world_name, &ctx.place_id, p.prefix.as_deref())
        .await?;

    Ok(Json(json!({
        "data": keys,
        "pagination": { "limit": p.limit, "offset": p.offset, "total": total }
    })))
}

/// DELETE /env — clear all env vars for the scene; owners and deployers only.
pub async fn clear(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;
    // Auth before the confirm-header check (upstream validates it inside the
    // handler, after the middleware chain) so unauthenticated callers get 401.
    require_confirm_delete_all(&headers)?;

    state
        .storage
        .env_delete_all(&ctx.world_name, &ctx.place_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
