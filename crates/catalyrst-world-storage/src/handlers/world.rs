use std::collections::HashMap;

use axum::extract::{FromRequest, Path, Query, State};
use axum::http::HeaderMap;

use crate::handlers::common::{
    check_content_length, get_value_response, parse_pagination, raw_paginated_response,
    raw_value_response, reject_nul_characters, require_confirm_delete_all, validate_key, RawJson,
    UpsertBody, ValidatedJson,
};
use crate::http::errors::ApiError;
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<RawJson, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    validate_key(&key)?;

    let value = state
        .storage
        .world_get(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    get_value_response(value)
}

pub async fn upsert(
    State(state): State<AppState>,
    Path(key): Path<String>,
    req: axum::extract::Request,
) -> Result<RawJson, ApiError> {
    let (parts, body) = req.into_parts();
    let path = signed_path(&parts.uri);
    let ctx = resolve_scene_context(&state, &parts.headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    validate_key(&key)?;
    check_content_length(&parts.headers, state.cfg.world_limits.max_value_size_bytes)?;

    let req = axum::http::Request::from_parts(parts, body);
    let ValidatedJson(body) = ValidatedJson::<UpsertBody>::from_request(req, &()).await?;

    let serialized = serde_json::to_string(&body.value)
        .map_err(|e| ApiError::bad_request(format!("invalid value: {e}")))?;
    reject_nul_characters(&serialized)?;

    state
        .storage
        .world_upsert_with_quota(
            &ctx.world_name,
            &ctx.place_id,
            &key,
            &serialized,
            state.cfg.world_limits,
        )
        .await?;
    Ok(raw_value_response(&serialized))
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
    validate_key(&key)?;

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
) -> Result<RawJson, ApiError> {
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

    Ok(raw_paginated_response(&entries, p.limit, p.offset, total))
}

pub async fn clear(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<axum::http::StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    require_confirm_delete_all(&headers)?;

    state
        .storage
        .world_delete_all(&ctx.world_name, &ctx.place_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
