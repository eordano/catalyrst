use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::handlers::common::{is_eth_address, normalize_player};
use crate::http::errors::ApiError;
use crate::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get_world_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let info = state.storage.world_size_info(&ctx.world_name, None).await?;
    Ok(Json(json!({
        "usedBytes": info.total_size,
        "maxTotalSizeBytes": state.cfg.world_limits.max_total_size_bytes,
    })))
}

pub async fn get_player_usage(
    State(state): State<AppState>,
    Path(player): Path<String>,
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

    let info = state
        .storage
        .player_size_info(&ctx.world_name, &player, None)
        .await?;
    Ok(Json(json!({
        "usedBytes": info.total_size,
        "maxTotalSizeBytes": state.cfg.player_limits.max_total_size_bytes,
    })))
}

pub async fn get_env_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<Value>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    let info = state.storage.env_size_info(&ctx.world_name, None).await?;
    Ok(Json(json!({
        "usedBytes": info.total_size,
        "maxTotalSizeBytes": state.cfg.env_limits.max_total_size_bytes,
    })))
}
