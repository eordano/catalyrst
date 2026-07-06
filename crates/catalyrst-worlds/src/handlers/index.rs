use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::ports::worlds::WorldScene;
use crate::AppState;

const MAX_INDEX_LIMIT: i64 = 10_000;

#[derive(Debug, Deserialize)]
pub struct IndexQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

pub async fn get_index(
    State(state): State<AppState>,
    Query(q): Query<IndexQuery>,
) -> Result<Json<Value>, ApiError> {
    let base_url = &state.cfg.http_base_url;

    let (limit, offset) = bound_index_params(q.limit, q.offset);

    let scenes = state.worlds.list_index_scenes(limit, offset).await?;

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

fn bound_index_params(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit
        .filter(|&l| l > 0)
        .map(|l| l.min(MAX_INDEX_LIMIT))
        .unwrap_or(MAX_INDEX_LIMIT);
    let offset = offset.filter(|&o| o >= 0).unwrap_or(0);
    (limit, offset)
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
    let runtime_version = scene
        .entity
        .get("metadata")
        .and_then(|m| m.get("runtimeVersion"))
        .cloned()
        .unwrap_or(Value::Null);

    json!({
        "id": scene.entity_id,
        "title": title,
        "description": description,
        "thumbnail": thumbnail,
        "pointers": scene.parcels,
        "runtimeVersion": runtime_version,
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

#[cfg(test)]
mod tests {
    use super::{bound_index_params, scene_summary, MAX_INDEX_LIMIT};
    use crate::ports::worlds::WorldScene;
    use serde_json::json;

    #[test]
    fn scene_summary_includes_runtime_version_from_metadata() {
        let scene = WorldScene {
            entity_id: "bafy-scene".into(),
            entity: json!({
                "timestamp": 111,
                "metadata": {
                    "runtimeVersion": "7",
                    "display": { "title": "Hello" }
                }
            }),
            parcels: vec!["0,0".into()],
        };
        let body = scene_summary(&scene, "https://worlds.example");
        assert_eq!(body["runtimeVersion"], json!("7"));
        assert_eq!(body["title"], json!("Hello"));
        assert_eq!(body["id"], json!("bafy-scene"));
    }

    #[test]
    fn scene_summary_runtime_version_is_null_when_absent() {
        let scene = WorldScene {
            entity_id: "s".into(),
            entity: json!({ "timestamp": 1, "metadata": { "display": {} } }),
            parcels: vec![],
        };
        let body = scene_summary(&scene, "https://x");
        assert!(body["runtimeVersion"].is_null());
    }

    #[test]
    fn index_params_default_when_absent() {
        assert_eq!(bound_index_params(None, None), (MAX_INDEX_LIMIT, 0));
    }

    #[test]
    fn index_limit_clamps_at_max() {
        assert_eq!(bound_index_params(Some(1_000_000), None).0, MAX_INDEX_LIMIT);
        assert_eq!(
            bound_index_params(Some(MAX_INDEX_LIMIT + 1), None).0,
            MAX_INDEX_LIMIT
        );
        assert_eq!(
            bound_index_params(Some(MAX_INDEX_LIMIT), None).0,
            MAX_INDEX_LIMIT
        );
        assert_eq!(bound_index_params(Some(50), None).0, 50);
        assert_eq!(bound_index_params(Some(0), None).0, MAX_INDEX_LIMIT);
        assert_eq!(bound_index_params(Some(-5), None).0, MAX_INDEX_LIMIT);
    }

    #[test]
    fn index_offset_parses_and_defaults() {
        assert_eq!(bound_index_params(None, Some(25)).1, 25);
        assert_eq!(bound_index_params(None, Some(0)).1, 0);
        assert_eq!(bound_index_params(None, Some(-1)).1, 0);
        assert_eq!(bound_index_params(None, Some(i64::MIN)).1, 0);
        assert_eq!(bound_index_params(Some(10), Some(20)), (10, 20));
    }
}
