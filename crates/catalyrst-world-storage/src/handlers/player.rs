use std::collections::HashMap;

use axum::extract::{FromRequest, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{
    check_content_length, get_value_response, is_eth_address, normalize_player, parse_pagination,
    raw_paginated_response, raw_value_response, reject_nul_characters, require_confirm_delete_all,
    validate_key, RawJson, UpsertBody, ValidatedJson,
};
use crate::http::errors::ApiError;
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get(
    State(state): State<AppState>,
    Path((player, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<RawJson, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }
    validate_key(&key)?;

    let value = state
        .storage
        .player_get(&ctx.world_name, &ctx.place_id, &player, &key)
        .await?;
    get_value_response(value)
}

// Authorization runs before the body is buffered or parsed so unauthorized callers
// cannot make the server do either; Content-Length is checked before buffering.
pub async fn upsert(
    State(state): State<AppState>,
    Path((player, key)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<RawJson, ApiError> {
    let player = normalize_player(&player)?;
    let (parts, body) = req.into_parts();
    let path = signed_path(&parts.uri);
    let ctx = resolve_scene_context(&state, &parts.headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }
    validate_key(&key)?;
    check_content_length(&parts.headers, state.cfg.player_limits.max_value_size_bytes)?;

    let req = axum::http::Request::from_parts(parts, body);
    let ValidatedJson(body) = ValidatedJson::<UpsertBody>::from_request(req, &()).await?;

    // Serialized once here; the same text is stored and spliced into the response.
    let serialized = serde_json::to_string(&body.value)
        .map_err(|e| ApiError::bad_request(format!("invalid value: {e}")))?;
    reject_nul_characters(&serialized)?;

    state
        .storage
        .player_upsert_with_quota(
            &ctx.world_name,
            &ctx.place_id,
            &player,
            &key,
            &serialized,
            state.cfg.player_limits,
        )
        .await?;
    Ok(raw_value_response(&serialized))
}

pub async fn delete(
    State(state): State<AppState>,
    Path((player, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }
    validate_key(&key)?;

    state
        .storage
        .player_delete(&ctx.world_name, &ctx.place_id, &player, &key)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list(
    State(state): State<AppState>,
    Path(player): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<RawJson, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }

    let p = parse_pagination(&params)?;
    let entries = state
        .storage
        .player_list(
            &ctx.world_name,
            &ctx.place_id,
            &player,
            p.limit,
            p.offset,
            p.prefix.as_deref(),
        )
        .await?;
    let total = state
        .storage
        .player_count(&ctx.world_name, &ctx.place_id, &player, p.prefix.as_deref())
        .await?;

    Ok(raw_paginated_response(&entries, p.limit, p.offset, total))
}

pub async fn clear(
    State(state): State<AppState>,
    Path(player): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<StatusCode, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "delete", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }

    require_confirm_delete_all(&headers)?;

    state
        .storage
        .player_delete_all_for_player(&ctx.world_name, &ctx.place_id, &player)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_players(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let p = parse_pagination(&params)?;
    let players = state
        .storage
        .player_list_players(&ctx.world_name, &ctx.place_id, p.limit, p.offset)
        .await?;
    let total = state
        .storage
        .player_count_players(&ctx.world_name, &ctx.place_id)
        .await?;

    Ok(Json(json!({
        "data": players,
        "pagination": { "limit": p.limit, "offset": p.offset, "total": total }
    })))
}

pub async fn clear_all_players(
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
        .player_delete_all(&ctx.world_name, &ctx.place_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
