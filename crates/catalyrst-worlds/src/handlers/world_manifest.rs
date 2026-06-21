use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::AppState;

pub async fn get_world_manifest(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state.name_denylist.check_name_deny_list(&world_name).await {
        return Err(ApiError::not_found(format!(
            "World \"{world_name}\" not found."
        )));
    }

    let manifest = state
        .worlds
        .get_world_manifest(&world_name)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!("World \"{world_name}\" has no scenes deployed."))
        })?;

    let spawn = manifest.spawn_coordinates.as_deref().unwrap_or("0,0");
    let mut parts = spawn.split(',');
    let x = parts.next().unwrap_or("0");
    let y = parts.next().unwrap_or("0");

    let body = json!({
        "occupied": manifest.parcels,
        "spawn_coordinate": { "x": x, "y": y },
        "total": manifest.total,
    });
    Ok(Json(body))
}
