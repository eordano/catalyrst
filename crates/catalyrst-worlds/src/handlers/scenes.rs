use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::auth_chain::require_verified;
use crate::fed::names::LocalWorldName;
use crate::handlers::deploy::canon_pointer;
use crate::handlers::permissions::{map_auth_error, resolve_world_owner};
use crate::http::ApiError;
use crate::AppState;

const PARCEL_PAGE: i64 = 100_000;

#[utoipa::path(
    get,
    path = "/world/{world_name}/scenes",
    tag = "scenes",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn list_scenes(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let scenes = state.worlds.list_scenes(&world_name).await?;
    let out: Vec<Value> = scenes
        .into_iter()
        .map(|(entity_id, parcels, base)| {
            let base = base.unwrap_or_default();
            json!({ "entityId": entity_id, "parcels": parcels, "baseParcel": base })
        })
        .collect();
    Ok(Json(json!({ "scenes": out })))
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
    let auth = require_verified(&headers, "delete", uri.path()).map_err(map_auth_error)?;
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

    if !is_owner {
        let records = state
            .worlds
            .get_world_permission_records_full(&world_name)
            .await?;
        let mut allowed = false;
        for r in records.iter().filter(|r| {
            r.permission_type == "deployment" && r.address.eq_ignore_ascii_case(&signer)
        }) {
            if r.is_world_wide {
                allowed = true;
                break;
            }
            let (_total, parcels) = state
                .worlds
                .get_parcels_for_permission(r.id, PARCEL_PAGE, 0, None)
                .await?;
            if parcels.iter().any(|p| canon_pointer(p) == parcel) {
                allowed = true;
                break;
            }
        }
        if !allowed {
            return Err(ApiError::forbidden(format!(
                "Your wallet can not unpublish scenes from \"{world_name}\"."
            )));
        }
    }

    let removed = state.worlds.undeploy_scene(&world_name, &parcel).await?;
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
    let auth = require_verified(&headers, "delete", uri.path()).map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();

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

    if !is_owner {
        let records = state
            .worlds
            .get_world_permission_records_full(&world_name)
            .await?;
        let has_world_wide = records.iter().any(|r| {
            r.permission_type == "deployment"
                && r.address.eq_ignore_ascii_case(&signer)
                && r.is_world_wide
        });
        if !has_world_wide {
            return Err(ApiError::forbidden(format!(
                "You must have world-wide deployment permission to undeploy \"{world_name}\"."
            )));
        }
    }

    let removed = state.worlds.undeploy_world(&world_name).await?;
    tracing::info!(world = %world_name, signer = %signer, removed, "world undeployed");
    Ok(Json(json!({})))
}
