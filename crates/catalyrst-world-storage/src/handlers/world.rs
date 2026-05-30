use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{
    is_js_falsy, parse_pagination, require_confirm_delete_all, UpsertBody,
};
use crate::http::errors::ApiError;
use crate::storage::{check_limits, value_size_bytes};
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let value = state
        .storage
        .world_get(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    match value {
        Some(v) if !is_js_falsy(&v) => Ok(Json(json!({ "value": v }))),
        _ => Err(ApiError::not_found("Value not found")),
    }
}

pub async fn upsert(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    Json(body): Json<UpsertBody>,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let serialized = serde_json::to_string(&body.value)
        .map_err(|e| ApiError::bad_request(format!("invalid value: {e}")))?;
    let size = value_size_bytes(&serialized);
    let info = state.storage.world_size_info(&ctx.world_name, Some(&key)).await?;
    check_limits(size, info, state.cfg.world_limits)?;

    let stored = state
        .storage
        .world_set(&ctx.world_name, &ctx.place_id, &key, &body.value, size)
        .await?;
    Ok(Json(json!({ "value": stored })))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<axum::http::StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    state
        .storage
        .world_delete(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let p = parse_pagination(&params)?;
    let entries = state
        .storage
        .world_list(
            &ctx.world_name,
            &ctx.place_id,
            p.limit,
            p.offset,
            p.prefix.as_deref(),
        )
        .await?;
    let total = state
        .storage
        .world_count(&ctx.world_name, &ctx.place_id, p.prefix.as_deref())
        .await?;

    let data: Vec<Value> = entries
        .into_iter()
        .map(|e| json!({ "key": e.key, "value": e.value }))
        .collect();
    Ok(Json(json!({
        "data": data,
        "pagination": { "limit": p.limit, "offset": p.offset, "total": total }
    })))
}

pub async fn clear(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<axum::http::StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;
    // Upstream checks the confirm header INSIDE the handler, i.e. after the
    // signed-fetch + authorization middleware. Keep auth first so an
    // unauthenticated caller gets 401, not a 400 that leaks the header name.
    require_confirm_delete_all(&headers)?;

    state
        .storage
        .world_delete_all(&ctx.world_name, &ctx.place_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
