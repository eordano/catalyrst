use std::collections::HashMap;

use axum::extract::{FromRequest, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{
    check_content_length, parse_pagination, require_confirm_delete_all, validate_key,
    UpsertEnvBody, ValidatedJson,
};
use crate::http::errors::ApiError;
use crate::storage::value_size_bytes;
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::AUTHORIZED_ADDRESSES_ONLY).await?;
    validate_key(&key)?;

    let enc = state
        .storage
        .env_get_enc(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    match enc {
        // An empty string is a legitimately stored value (the upsert schema accepts
        // it), so only a missing row is a 404.
        Some(blob) => {
            let value = state.encryptor.decrypt(&blob)?;
            Ok(Json(json!({ "value": value })))
        }
        None => Err(ApiError::not_found("Value not found")),
    }
}

// Authorization runs before the body is buffered or parsed so unauthorized callers
// cannot make the server do either; Content-Length is checked before buffering.
pub async fn upsert(
    State(state): State<AppState>,
    Path(key): Path<String>,
    req: axum::extract::Request,
) -> Result<StatusCode, ApiError> {
    let (parts, body) = req.into_parts();
    let path = signed_path(&parts.uri);
    let ctx = resolve_scene_context(&state, &parts.headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;
    validate_key(&key)?;
    check_content_length(&parts.headers, state.cfg.env_limits.max_value_size_bytes)?;

    let req = axum::http::Request::from_parts(parts, body);
    let ValidatedJson(body) = ValidatedJson::<UpsertEnvBody>::from_request(req, &()).await?;

    let size = value_size_bytes(&body.value);
    let enc = state.encryptor.encrypt(&body.value)?;
    state
        .storage
        .env_upsert_with_quota(
            &ctx.world_name,
            &ctx.place_id,
            &key,
            &enc,
            size,
            state.cfg.env_limits,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;
    validate_key(&key)?;

    state
        .storage
        .env_delete(&ctx.world_name, &ctx.place_id, &key)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

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

pub async fn clear(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    require_confirm_delete_all(&headers)?;

    state
        .storage
        .env_delete_all(&ctx.world_name, &ctx.place_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
