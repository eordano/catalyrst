use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::ports::worlds::WorldScene;
use crate::AppState;

pub async fn get_index(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let base_url = &state.cfg.http_base_url;
    let scenes = state.worlds.list_index_scenes().await?;

    let mut data: Vec<Value> = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_scenes: Vec<Value> = Vec::new();
    for (world_name, scene) in scenes {
        if cur_name.as_deref() != Some(world_name.as_str()) {
            if let Some(name) = cur_name.take() {
                data.push(json!({ "name": name, "scenes": std::mem::take(&mut cur_scenes) }));
            }
            cur_name = Some(world_name);
        }
        cur_scenes.push(scene_summary(&scene, base_url));
    }
    if let Some(name) = cur_name.take() {
        data.push(json!({ "name": name, "scenes": cur_scenes }));
    }

    Ok(Json(json!({ "data": data })))
}

fn scene_summary(scene: &WorldScene, base_url: &str) -> Value {
    let display = scene.entity.get("metadata").and_then(|m| m.get("display"));
    let title = display
        .and_then(|d| d.get("title"))
        .and_then(|v| v.as_str());
    let description = display
        .and_then(|d| d.get("description"))
        .and_then(|v| v.as_str());
    let thumbnail = display
        .and_then(|d| d.get("navmapThumbnail"))
        .and_then(|v| v.as_str())
        .and_then(|file| resolve_content(&scene.entity, file))
        .map(|hash| format!("{}/contents/{}", base_url, hash));
    let timestamp = scene
        .entity
        .get("timestamp")
        .cloned()
        .unwrap_or(Value::Null);

    json!({
        "id": scene.entity_id,
        "title": title,
        "description": description,
        "thumbnail": thumbnail,
        "pointers": scene.parcels,
        "timestamp": timestamp,
    })
}

fn resolve_content(entity: &Value, file: &str) -> Option<String> {
    entity
        .get("content")?
        .as_array()?
        .iter()
        .find(|c| c.get("file").and_then(|f| f.as_str()) == Some(file))
        .and_then(|c| c.get("hash").and_then(|h| h.as_str()))
        .map(|s| s.to_string())
}
