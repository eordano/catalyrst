use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use crate::http::ApiError;
use crate::ports::worlds::WorldScene;
use crate::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct MinimapConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub data_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub estate_image: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct SkyboxConfig {
    pub fixed_hour: Option<f64>,
    pub textures: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutMap {
    pub minimap_enabled: bool,
    #[cfg_attr(feature = "ts", ts(type = "unknown[]"))]
    pub sizes: Vec<Value>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutConfigurations {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub network_id: i64,
    pub global_scenes_urn: Vec<String>,
    pub scenes_urn: Vec<String>,
    pub minimap: MinimapConfig,
    pub skybox: SkyboxConfig,
    pub realm_name: String,
    pub map: AboutMap,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutContentStatus {
    pub synchronization_status: String,
    pub healthy: bool,
    pub public_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutServiceStatus {
    pub healthy: bool,
    pub public_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutComms {
    pub healthy: bool,
    pub protocol: String,
    pub adapter: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct AboutResponse {
    pub healthy: bool,
    pub accepting_users: bool,
    pub spawn_coordinates: Option<String>,
    pub configurations: AboutConfigurations,
    pub content: AboutContentStatus,
    pub lambdas: AboutServiceStatus,
    pub comms: AboutComms,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub catalyrst: Value,
}

#[utoipa::path(
    get,
    path = "/world/{world_name}/about",
    tag = "worlds",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = AboutResponse),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_about(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<AboutResponse>, ApiError> {
    let cfg = &state.cfg;

    if !state.name_denylist.check_name_deny_list(&world_name).await {
        return Err(ApiError::not_found(format!(
            "World \"{}\" has no scene deployed.",
            world_name
        )));
    }

    let world = state.worlds.get_world(&world_name).await?;
    let scenes = state.worlds.get_scenes(&world_name).await?;

    if scenes.is_empty() {
        return Err(ApiError::not_found(format!(
            "World \"{}\" has no scenes deployed.",
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
    let mut rt = RuntimeMeta::from_scene(&world_name, primary);

    if let Some(w) = &world {
        if let Some(t) = w.skybox_time {
            rt.skybox_fixed_time = Some(t as f64);
        }
        if w.single_player {
            rt.fixed_adapter = Some("offline:offline".to_string());
        }
    }

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

    let url_for_file = |filename: &Option<String>, default_image: &str| -> String {
        match filename {
            Some(f) => format!("{}/contents/{}", base_url, f),
            None => default_image.to_string(),
        }
    };
    let data_image = if rt.minimap_visible || rt.minimap_data_image.is_some() {
        let data_default = std::env::var("MAP_PARCEL_VIEW_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:5162/v1/minimap.png".to_string());
        Some(url_for_file(&rt.minimap_data_image, &data_default))
    } else {
        None
    };
    let estate_image = if rt.minimap_visible || rt.minimap_estate_image.is_some() {
        let estate_default = std::env::var("MAP_ESTATE_VIEW_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:5162/v1/estatemap.png".to_string());
        Some(url_for_file(&rt.minimap_estate_image, &estate_default))
    } else {
        None
    };
    let minimap = MinimapConfig {
        enabled: rt.minimap_visible,
        data_image,
        estate_image,
    };

    let skybox = SkyboxConfig {
        fixed_hour: rt.skybox_fixed_time,
        textures: rt
            .skybox_textures
            .iter()
            .map(|t| format!("{}/contents/{}", base_url, t))
            .collect(),
    };

    let map = AboutMap {
        minimap_enabled: false,
        sizes: Vec::new(),
    };

    let adapter = resolve_fixed_adapter(&world_name, rt.fixed_adapter.as_deref(), base_url);

    let content_healthy = true;
    let lambdas_healthy = true;
    let healthy = content_healthy && lambdas_healthy;

    Ok(Json(AboutResponse {
        healthy,
        accepting_users: healthy,
        spawn_coordinates,
        configurations: AboutConfigurations {
            network_id: cfg.network_id,
            global_scenes_urn,
            scenes_urn,
            minimap,
            skybox,
            realm_name: rt.name,
            map,
        },
        content: AboutContentStatus {
            synchronization_status: "Syncing".to_string(),
            healthy: content_healthy,
            public_url: cfg.content_public_url.clone(),
        },
        lambdas: AboutServiceStatus {
            healthy: lambdas_healthy,
            public_url: cfg.lambdas_public_url.clone(),
        },
        comms: AboutComms {
            healthy: true,
            protocol: "v3".to_string(),
            adapter,
        },
        catalyrst: catalyrst_extensions(base_url),
    }))
}

/// Every route this node serves that a stock catalyst does not.
///
/// `/about` is the only thing a client fetches before it can do anything else,
/// so anything not named here has to be hardcoded by every client — and a
/// hardcoded path silently points at whichever node the constant was written
/// for. That is not hypothetical: bevy-explorer shipped
/// `catalyst.example.com/comms/get-scene-adapter`, so on this node it minted scene
/// rooms against a different federation entirely, joined a LiveKit the
/// authoritative server was not in, and every scene showed "Server
/// Disconnected" while looking healthy from the outside.
///
/// Shape follows the catalyst service blocks above (`healthy` + `publicUrl`),
/// under one additive key: a stock client ignores what it does not know, and a
/// catalyrst-aware one discovers the whole surface from a single fetch.
fn catalyrst_extensions(base_url: &str) -> serde_json::Value {
    let at = |path: &str| json!({ "healthy": true, "publicUrl": format!("{base_url}{path}") });

    json!({
        "healthy": true,
        // Per-scene LiveKit rooms. `sceneAdapter` is the client mint (ADR-44
        // signed fetch); `serverSceneAdapter` is the authoritative scene runner
        // mint, which answers only to the configured server identity.
        "sceneAdapter": at("/get-scene-adapter"),
        "serverSceneAdapter": at("/get-server-scene-adapter"),
        "worldStorage": at("/world-storage"),
        "sceneState": at("/scene-state"),
        "places": at("/places"),
        "events": at("/events"),
        "marketplace": at("/marketplace"),
        "communities": at("/social/communities"),
        "socialRpc": at("/social-rpc"),
        "badges": at("/badges"),
        "telemetry": at("/telemetry"),
        "federation": at("/federation/communities"),
    })
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
            .and_then(&resolve);
        let minimap_estate_image = mini_map
            .and_then(|m| m.get("estateImage"))
            .and_then(|v| v.as_str())
            .and_then(&resolve);

        let skybox_textures = skybox
            .and_then(|s| s.get("textures"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str())
                    .filter_map(resolve)
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

#[cfg(test)]
mod tests {
    use super::resolve_fixed_adapter;

    #[test]
    fn single_player_override_maps_to_offline_adapter() {
        assert_eq!(
            resolve_fixed_adapter("foo", Some("offline:offline"), "https://worlds.example"),
            "fixed-adapter:offline:offline"
        );
    }

    #[test]
    fn no_override_falls_back_to_signed_login() {
        assert_eq!(
            resolve_fixed_adapter("Foo", None, "https://worlds.example"),
            "fixed-adapter:signed-login:https://worlds.example/worlds/foo/comms"
        );
    }

    #[test]
    fn extensions_are_absolute_urls_on_this_node() {
        let ext = super::catalyrst_extensions("https://worlds.example");

        // The whole point is that a client never has to compose one of these
        // itself, so every entry must be a complete url on the node that
        // answered — a bare path would leave the caller guessing the origin,
        // which is the bug this advert exists to retire.
        let entries = ext.as_object().expect("extensions is an object");
        assert!(entries.len() > 1, "advert is empty");
        for (name, block) in entries {
            if name == "healthy" {
                continue;
            }
            let url = block["publicUrl"]
                .as_str()
                .unwrap_or_else(|| panic!("{name} has no publicUrl"));
            assert!(
                url.starts_with("https://worlds.example/"),
                "{name} must be absolute on this node, got {url}"
            );
        }

        assert_eq!(
            ext["sceneAdapter"]["publicUrl"],
            "https://worlds.example/get-scene-adapter"
        );
    }
}
