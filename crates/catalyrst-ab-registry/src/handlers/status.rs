use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;

use crate::http::errors::{ApiError, ApiResult};
use crate::ports::manifest_store::AbManifests;
use crate::types::{EntityStatus, PlatformStatuses, WorldNameQuery};
use crate::AppState;

pub async fn service_status() -> Json<serde_json::Value> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Json(json!({
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "currentTime": current_time,
            "commitHash": option_env!("GIT_COMMIT").unwrap_or("unknown"),
        }
    }))
}

pub async fn get_entity_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<WorldNameQuery>,
) -> ApiResult<Json<EntityStatus>> {
    let resolved = if let Some(world) = q.world_name.as_deref() {
        let ents = state
            .content
            .resolve_pointers(std::slice::from_ref(&id))
            .await?;
        ents.into_iter().find(|e| {
            e.world_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(world))
        })
    } else {
        state.content.resolve_one(&id).await?
    };

    let ent = resolved.ok_or_else(|| ApiError::not_found("entity not found"))?;
    let m = state.manifests.get(&ent.entity_id).await;
    Ok(Json(entity_status_from(&ent.entity_id, &m, ent.is_world())))
}

pub async fn get_entities_status_signed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<EntityStatus>>> {
    let signer = require_signed_fetch(&headers, "get", "/entities/status")?;

    let ents = state.content.active_entities_by_deployer(&signer).await?;
    let mut out = Vec::with_capacity(ents.len());
    for ent in ents {
        let m = state.manifests.get(&ent.entity_id).await;
        out.push(entity_status_from(&ent.entity_id, &m, ent.is_world()));
    }
    Ok(Json(out))
}

pub(crate) fn entity_status_from(entity_id: &str, m: &AbManifests, is_world: bool) -> EntityStatus {
    use crate::types::BuildStatus;
    let asset_bundles = PlatformStatuses {
        mac: m.mac_status(),
        windows: m.windows_status(),
    };
    let complete = matches!(asset_bundles.mac, BuildStatus::Complete)
        && matches!(asset_bundles.windows, BuildStatus::Complete);
    let lods = if is_world {
        None
    } else {
        Some(PlatformStatuses {
            mac: m.lods.mac.unwrap_or(BuildStatus::Pending),
            windows: m.lods.windows.unwrap_or(BuildStatus::Pending),
        })
    };
    EntityStatus {
        entity_id: entity_id.to_string(),
        catalyst: BuildStatus::Complete,
        complete,
        asset_bundles,
        lods,
    }
}

pub(crate) fn require_signed_fetch(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, ApiError> {
    if let Some(signer) = headers
        .get("x-identity-metadata-signer")
        .and_then(|v| v.to_str().ok())
    {
        if signer == "decentraland-kernel-scene" {
            return Err(ApiError::unauthorized(
                "decentraland-kernel-scene signer is not allowed",
            ));
        }
    }

    catalyrst_comms::auth_chain::require_signer(headers, method, path)
        .map(|s| s.to_lowercase())
        .map_err(|e| ApiError::unauthorized(format!("signed-fetch verification failed: {e}")))
}
