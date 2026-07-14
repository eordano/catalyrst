use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::world_storage::dto::UsageResponse;
use crate::world_storage::handlers::common::{is_eth_address, normalize_player};
use crate::world_storage::http::errors::ApiError;
use crate::world_storage::{authorize, resolve_scene_context, signed_path, AppState, AuthPolicy};

pub async fn get_world_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<UsageResponse>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    let info = state
        .storage
        .world_size_info(&ctx.world_name, &ctx.place_id, None)
        .await?;
    Ok(Json(UsageResponse {
        used_bytes: info.total_size,
        max_total_size_bytes: state.cfg.world_limits.max_total_size_bytes,
    }))
}

pub async fn get_player_usage(
    State(state): State<AppState>,
    Path(player): Path<String>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<UsageResponse>, ApiError> {
    let player = normalize_player(&player)?;
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::DEFAULT).await?;

    if !is_eth_address(&player) {
        return Err(ApiError::bad_request("Invalid player address"));
    }

    let info = state
        .storage
        .player_size_info(&ctx.world_name, &ctx.place_id, &player, None)
        .await?;
    Ok(Json(UsageResponse {
        used_bytes: info.total_size,
        max_total_size_bytes: state.cfg.player_limits.max_total_size_bytes,
    }))
}

pub async fn get_env_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> Result<Json<UsageResponse>, ApiError> {
    let path = signed_path(&uri);
    let ctx = resolve_scene_context(&state, &headers, "get", &path).await?;
    authorize(&state, &ctx, AuthPolicy::OWNERS_DEPLOYERS_ONLY).await?;

    let info = state
        .storage
        .env_size_info(&ctx.world_name, &ctx.place_id, None)
        .await?;
    Ok(Json(UsageResponse {
        used_bytes: info.total_size,
        max_total_size_bytes: state.cfg.env_limits.max_total_size_bytes,
    }))
}
