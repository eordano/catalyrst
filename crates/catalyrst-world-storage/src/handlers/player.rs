use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{
    get_value_response, is_eth_address, normalize_player, parse_pagination,
    require_confirm_delete_all, UpsertBody, ValidatedJson,
};
use crate::http::errors::ApiError;
use crate::storage::{check_limits, value_size_bytes};
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get(
    State(state): State<AppState>,
    Path((player, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }

    let value = state
        .storage
        .player_get(&ctx.world_name, &ctx.place_id, &player, &key)
        .await?;
    get_value_response(value)
}

pub async fn upsert(
    State(state): State<AppState>,
    Path((player, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: axum::http::Uri,
    ValidatedJson(body): ValidatedJson<UpsertBody>,
) -> Result<Json<Value>, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "put", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;
    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }

    let serialized = serde_json::to_string(&body.value)
        .map_err(|e| ApiError::bad_request(format!("invalid value: {e}")))?;
    let size = value_size_bytes(&serialized);
    let info = state
        .storage
        .player_size_info(&ctx.world_name, &player, Some(&key))
        .await?;
    check_limits(size, info, state.cfg.player_limits)?;

    let stored = state
        .storage
        .player_set(
            &ctx.world_name,
            &ctx.place_id,
            &player,
            &key,
            &body.value,
            size,
        )
        .await?;
    Ok(Json(json!({ "value": stored })))
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
) -> Result<Json<Value>, ApiError> {
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
