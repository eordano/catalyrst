use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::AppState;

pub async fn get_permissions(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let world = state.worlds.get_world(&world_name).await?;
    let access = world.as_ref().map(|w| w.access.clone()).unwrap_or_default();
    let owner = world.as_ref().and_then(|w| w.owner.clone());

    let records = state.worlds.get_permission_records(&world_name).await?;

    let mut deployment_wallets: Vec<String> = Vec::new();
    let mut streaming_wallets: Vec<String> = Vec::new();
    let mut summary: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for (address, permission_type) in records {
        match permission_type.as_str() {
            "deployment" => deployment_wallets.push(address.clone()),
            "streaming" => streaming_wallets.push(address.clone()),
            _ => {}
        }
        summary.entry(address).or_default().push(json!({
            "permission": permission_type,
            "world_wide": true,
        }));
    }

    let body = json!({
        "permissions": {
            "deployment": { "type": "allow-list", "wallets": deployment_wallets },
            "streaming": { "type": "allow-list", "wallets": streaming_wallets },
            "access": access.to_public_json(),
        },
        "owner": owner,
        "summary": summary,
    });

    Ok(Json(body))
}
