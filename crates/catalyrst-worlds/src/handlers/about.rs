use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::ports::worlds::WorldScene;
use crate::AppState;

pub async fn get_about(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let cfg = &state.cfg;

    let world = state.worlds.get_world(&world_name).await?;
    let scenes = state.worlds.get_scenes(&world_name).await?;

    if scenes.is_empty() {
        return Err(ApiError::not_found(format!(
            "World \"{}\" has no scene deployed.",
            world_name
        )));
    }

    if let Some(w) = &world {
        if let Some(since) = w.blocked_since {
            return Err(ApiError::unauthorized(format!(
                "World \"{}\" has been blocked since {} as it exceeded its allowed storage space.",
                world_name, since
            )));
        }
    }

    let base_url = &cfg.http_base_url;
    let entity_ids: Vec<&str> = scenes.iter().map(|s| s.entity_id.as_str()).collect();

    let scenes_urn: Vec<String> = entity_ids
        .iter()
        .map(|id| {
            format!(
                "urn:decentraland:entity:{}?=&baseUrl={}/contents/",
                id, base_url
            )
        })
        .collect();

    let primary = &scenes[0];
    let rt = RuntimeMeta::from_scene(&world_name, primary);

    let spawn_coordinates = world
        .as_ref()
        .and_then(|w| w.spawn_coordinates.clone())
        .or_else(|| {
            primary
                .entity
                .get("metadata")
                .and_then(|m| m.get("scene"))
                .and_then(|s| s.get("base"))
                .and_then(|b| b.as_str())
                .map(|s| s.to_string())
        });

    let global_scenes_urn: Vec<String> = cfg
        .global_scenes_urn
        .as_deref()
        .map(|s| s.split_whitespace().map(|x| x.to_string()).collect())
        .unwrap_or_default();

    let mut minimap = serde_json::Map::new();
    minimap.insert("enabled".into(), json!(rt.minimap_visible));
    let url_for_file = |filename: &Option<String>, default_image: &str| -> String {
        match filename {
            Some(f) => format!("{}/contents/{}", base_url, f),
            None => default_image.to_string(),
        }
    };
    if rt.minimap_visible || rt.minimap_data_image.is_some() {
        let data_default = std::env::var("MAP_PARCEL_VIEW_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.decentraland.org/v1/minimap.png".to_string());
        minimap.insert(
            "dataImage".into(),
            json!(url_for_file(&rt.minimap_data_image, &data_default)),
        );
    }
    if rt.minimap_visible || rt.minimap_estate_image.is_some() {
        let estate_default = std::env::var("MAP_ESTATE_VIEW_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.decentraland.org/v1/estatemap.png".to_string());
        minimap.insert(
            "estateImage".into(),
            json!(url_for_file(&rt.minimap_estate_image, &estate_default)),
        );
    }
    let minimap = Value::Object(minimap);

    let skybox = json!({
        "fixedHour": rt.skybox_fixed_time,
        "textures": rt.skybox_textures
            .iter()
            .map(|t| format!("{}/contents/{}", base_url, t))
            .collect::<Vec<_>>(),
    });

    let map = json!({ "minimapEnabled": false, "sizes": [] });

    let adapter = resolve_fixed_adapter(&world_name, rt.fixed_adapter.as_deref(), base_url);

    let content_healthy = true;
    let lambdas_healthy = true;
    let healthy = content_healthy && lambdas_healthy;

    let body = json!({
        "healthy": healthy,
        "acceptingUsers": healthy,
        "spawnCoordinates": spawn_coordinates,
        "configurations": {
            "networkId": cfg.network_id,
            "globalScenesUrn": global_scenes_urn,
            "scenesUrn": scenes_urn,
            "minimap": minimap,
            "skybox": skybox,
            "realmName": rt.name,
            "map": map,
        },
        "content": {
            "synchronizationStatus": "Syncing",
            "healthy": content_healthy,
            "publicUrl": cfg.content_public_url,
        },
        "lambdas": {
            "healthy": lambdas_healthy,
            "publicUrl": cfg.lambdas_public_url,
        },
        "comms": {
            "healthy": true,
            "protocol": "v3",
            "adapter": adapter,
        },
    });

    Ok(Json(body))
}

fn resolve_fixed_adapter(world_name: &str, fixed_adapter: Option<&str>, base_url: &str) -> String {
    if fixed_adapter == Some("offline:offline") {
        return "fixed-adapter:offline:offline".to_string();
    }
    let url = format!("{}/worlds/{}/comms", base_url, world_name.to_lowercase());
    if base_url.starts_with("http://") {
        return url;
    }
    format!("fixed-adapter:signed-login:{}", url)
}

struct RuntimeMeta {
    name: String,
    minimap_visible: bool,
    minimap_data_image: Option<String>,
    minimap_estate_image: Option<String>,
    skybox_fixed_time: Option<f64>,
    skybox_textures: Vec<String>,
    fixed_adapter: Option<String>,
}

impl RuntimeMeta {
    fn from_scene(world_name: &str, scene: &WorldScene) -> Self {
        let wc = scene
            .entity
            .get("metadata")
            .and_then(|m| m.get("worldConfiguration"));

        let name = wc
            .and_then(|c| c.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| world_name.to_string());

        let minimap_visible = wc
            .and_then(|c| c.get("miniMapConfig"))
            .and_then(|m| m.get("visible"))
            .and_then(|v| v.as_bool())
            .or_else(|| {
                wc.and_then(|c| c.get("minimapVisible"))
                    .and_then(|v| v.as_bool())
            })
            .unwrap_or(false);

        let mini_map = wc.and_then(|c| c.get("miniMapConfig"));

        let skybox = wc.and_then(|c| c.get("skyboxConfig"));
        let skybox_fixed_time = skybox
            .and_then(|s| s.get("fixedTime"))
            .and_then(|v| v.as_f64());

        let content = scene.entity.get("content").and_then(|c| c.as_array());
        let resolve = |filename: &str| -> Option<String> {
            content.and_then(|arr| {
                arr.iter()
                    .find(|c| c.get("file").and_then(|f| f.as_str()) == Some(filename))
                    .and_then(|c| c.get("hash").and_then(|h| h.as_str()))
                    .map(|s| s.to_string())
            })
        };

        let minimap_data_image = mini_map
            .and_then(|m| m.get("dataImage"))
            .and_then(|v| v.as_str())
            .and_then(|f| resolve(f));
        let minimap_estate_image = mini_map
            .and_then(|m| m.get("estateImage"))
            .and_then(|v| v.as_str())
            .and_then(|f| resolve(f));

        let skybox_textures = skybox
            .and_then(|s| s.get("textures"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str())
                    .filter_map(|t| resolve(t))
                    .collect()
            })
            .unwrap_or_default();

        let fixed_adapter = wc
            .and_then(|c| c.get("fixedAdapter"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        RuntimeMeta {
            name,
            minimap_visible,
            minimap_data_image,
            minimap_estate_image,
            skybox_fixed_time,
            skybox_textures,
            fixed_adapter,
        }
    }
}
