use std::collections::HashSet;

use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::SecondsFormat;
use serde::Serialize;
use serde_json::{json, Value};

use crate::auth_chain::require_verified;
use crate::fed::names::LocalWorldName;
use crate::handlers::deploy::canon_pointer;
use crate::handlers::permissions::{map_auth_error, resolve_world_owner};
use crate::http::ApiError;
use crate::ports::worlds::WorldProbe;
use crate::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct WorldSceneEntry {
    pub world_name: String,
    pub deployer: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown[]"))]
    pub deployment_auth_chain: Value,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub entity: Value,
    pub entity_id: String,
    pub parcels: Vec<String>,
    pub size: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct SceneListResponse {
    pub scenes: Vec<WorldSceneEntry>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

#[utoipa::path(
    get,
    path = "/world/{world_name}/scenes",
    tag = "scenes",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = SceneListResponse),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn list_scenes(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<SceneListResponse>, ApiError> {
    let rows = state.worlds.list_scenes_full(&world_name).await?;
    let scenes: Vec<WorldSceneEntry> = rows
        .into_iter()
        .map(|r| WorldSceneEntry {
            world_name: r.world_name,
            deployer: r.deployer,
            deployment_auth_chain: r.deployment_auth_chain,
            entity: r.entity,
            entity_id: r.entity_id,
            parcels: r.parcels,
            size: r.size.to_string(),
            status: "DEPLOYED".to_string(),
            created_at: r.created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
            updated_at: r.updated_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        })
        .collect();
    let total = scenes.len() as i64;
    Ok(Json(SceneListResponse { scenes, total }))
}

#[utoipa::path(
    delete,
    path = "/world/{world_name}/scenes/{scene_coord}",
    tag = "scenes",
    params(("world_name" = String, Path), ("scene_coord" = String, Path)),
    responses(
        (status = 200),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_scene(
    State(state): State<AppState>,
    Path((world_name, scene_coord)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let auth = require_verified(&headers, "delete", uri.path(), &[])
        .await
        .map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();
    let parcel = canon_pointer(&scene_coord);

    let world = state.worlds.get_world(&world_name).await?;
    let owner = resolve_world_owner(
        &state,
        &LocalWorldName::from_request_path(&world_name),
        world.and_then(|w| w.owner),
    )
    .await;
    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);

    let authorized_entity_ids: Option<Vec<String>> = if is_owner {
        None
    } else {
        let overlapping = state
            .worlds
            .scenes_overlapping_parcels(&world_name, std::slice::from_ref(&parcel))
            .await?;
        let mut required: HashSet<String> = std::iter::once(parcel.clone()).collect();
        for scene in &overlapping {
            for p in &scene.parcels {
                required.insert(canon_pointer(p));
            }
        }
        let entity_ids: Vec<String> = overlapping.iter().map(|s| s.entity_id.clone()).collect();

        let required: Vec<String> = required.into_iter().collect();
        let allowed = state
            .worlds
            .has_deployment_permission_covering(&world_name, &signer, &required)
            .await?;
        if !allowed {
            return Err(ApiError::forbidden(format!(
                "Your wallet can not unpublish scenes from \"{world_name}\"."
            )));
        }
        Some(entity_ids)
    };

    let removed = state
        .worlds
        .undeploy_scene(&world_name, &parcel, authorized_entity_ids.as_deref())
        .await?;
    if removed == 0 {
        return Err(ApiError::not_found(format!(
            "No scene is published at {parcel} in \"{world_name}\"."
        )));
    }
    Ok(StatusCode::OK)
}

#[utoipa::path(
    delete,
    path = "/entities/{world_name}",
    tag = "entities",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn undeploy_world(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let auth = require_verified(&headers, "delete", uri.path(), &[])
        .await
        .map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();

    let lookup = state
        .worlds
        .lookup_world(
            &world_name,
            WorldProbe {
                world_wide_deployer: Some(&signer),
                ..WorldProbe::default()
            },
        )
        .await?;
    let owner = resolve_world_owner(
        &state,
        &LocalWorldName::from_request_path(&world_name),
        lookup.world.and_then(|w| w.owner),
    )
    .await;
    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);

    if !is_owner && !lookup.world_wide_deployer {
        return Err(ApiError::forbidden(format!(
            "You must have world-wide deployment permission to undeploy \"{world_name}\"."
        )));
    }

    let removed = state.worlds.undeploy_world(&world_name).await?;
    tracing::info!(world = %world_name, signer = %signer, removed, "world undeployed");
    Ok(Json(json!({})))
}
