use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::admin::authorize_admin;
use crate::http::ApiError;
use crate::AppState;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

fn clamp_page(q: &PageQuery) -> (i64, i64) {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);
    (limit, offset)
}

pub async fn list_worlds(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let (limit, offset) = clamp_page(&q);
    let total = state.worlds.admin_count_worlds().await?;
    let rows = state.worlds.admin_list_worlds(limit, offset).await?;
    let worlds: Vec<Value> = rows
        .into_iter()
        .map(|w| {
            json!({
                "name": w.name,
                "owner": w.owner,
                "accessType": w.access_type,
                "blocked": w.blocked_since.is_some(),
                "blockedSince": w.blocked_since.map(|t| t.to_rfc3339()),
                "spawnCoordinates": w.spawn_coordinates,
                "sceneCount": w.scene_count,
            })
        })
        .collect();
    Ok(Json(json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "worlds": worlds,
    })))
}

pub async fn world_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let world =
        state.worlds.get_world(&world_name).await?.ok_or_else(|| {
            ApiError::not_found(format!("World \"{}\" was not found.", world_name))
        })?;
    let scenes = state.worlds.get_scenes(&world_name).await?;
    let perms = state.worlds.get_permission_records(&world_name).await?;
    let permissions: Vec<Value> = perms
        .into_iter()
        .map(|(address, permission_type)| json!({ "address": address, "type": permission_type }))
        .collect();
    Ok(Json(json!({
        "name": world.name,
        "owner": world.owner,
        "access": world.access.to_public_json(),
        "blocked": world.blocked_since.is_some(),
        "blockedSince": world.blocked_since.map(|t| t.to_rfc3339()),
        "spawnCoordinates": world.spawn_coordinates,
        "sceneCount": scenes.len(),
        "scenes": scenes.iter().map(|s| json!({
            "entityId": s.entity_id,
            "parcels": s.parcels,
        })).collect::<Vec<_>>(),
        "permissions": permissions,
    })))
}

pub async fn disable_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    set_world_blocked(&state, &world_name, true).await
}

pub async fn enable_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    set_world_blocked(&state, &world_name, false).await
}

async fn set_world_blocked(
    state: &AppState,
    world_name: &str,
    blocked: bool,
) -> Result<Json<Value>, ApiError> {
    let updated = state
        .worlds
        .admin_set_world_blocked(world_name, blocked)
        .await?;
    if !updated {
        return Err(ApiError::not_found(format!(
            "World \"{}\" was not found.",
            world_name
        )));
    }
    Ok(Json(
        json!({ "ok": true, "world": world_name, "blocked": blocked }),
    ))
}

pub async fn list_blocked(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let rows = state.worlds.admin_list_blocked().await?;
    let blocked: Vec<Value> = rows
        .into_iter()
        .map(|b| {
            json!({
                "wallet": b.wallet,
                "createdAt": b.created_at.to_rfc3339(),
                "updatedAt": b.updated_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(json!({ "blocked": blocked })))
}

pub async fn block_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(wallet): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    state.worlds.admin_block_wallet(&wallet).await?;
    Ok(Json(
        json!({ "ok": true, "wallet": wallet.to_lowercase(), "blocked": true }),
    ))
}

pub async fn unblock_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(wallet): Path<String>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let removed = state.worlds.admin_unblock_wallet(&wallet).await?;
    if !removed {
        return Err(ApiError::not_found(format!(
            "Wallet \"{}\" is not on the block list.",
            wallet
        )));
    }
    Ok(Json(
        json!({ "ok": true, "wallet": wallet.to_lowercase(), "blocked": false }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct AccessLogQuery {
    #[serde(default)]
    pub world: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

pub async fn access_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AccessLogQuery>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows = state
        .worlds
        .admin_query_access_log(q.world.as_deref(), q.address.as_deref(), limit, offset)
        .await?;
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "worldName": r.world_name,
                "address": r.address,
                "action": r.action,
                "room": r.room,
                "createdAt": r.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(json!({
        "limit": limit,
        "offset": offset,
        "entries": entries,
    })))
}

#[derive(Debug, Deserialize)]
pub struct BanStatusQuery {
    pub address: String,
    #[serde(default)]
    pub parcel: Option<String>,
}

pub async fn world_ban_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_name): Path<String>,
    Query(q): Query<BanStatusQuery>,
) -> Result<Json<Value>, ApiError> {
    authorize_admin(&state, &headers)?;
    if !state.bans.is_configured() {
        return Err(ApiError::service_unavailable(
            "comms-gatekeeper not configured",
        ));
    }
    let platform_banned = state.bans.is_player_banned(&q.address).await;
    let scene_banned = match q.parcel.as_deref() {
        Some(parcel) => Some(
            state
                .bans
                .is_user_banned_from_scene(&q.address, &world_name, parcel)
                .await,
        ),
        None => None,
    };
    Ok(Json(json!({
        "world": world_name,
        "address": q.address.to_lowercase(),
        "platformBanned": platform_banned,
        "parcel": q.parcel,
        "sceneBanned": scene_banned,
    })))
}
