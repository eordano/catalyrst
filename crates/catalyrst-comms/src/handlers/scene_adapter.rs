use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth_chain::{try_extract_signer, verify_signed_fetch};
use crate::extract::{device_identifier, get_request_ip};
use crate::http::{auth_error, forbidden, unauthorized, ApiError};
use crate::livekit::{
    build_adapter_url, scene_room_name, world_scene_room_name, AccessToken, VideoGrants,
};
use crate::ports::player_connection::UpsertPlayerConnection;
use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct SceneAdapterRequest {
    #[serde(rename = "sceneId")]
    pub scene_id: Option<String>,
    pub identity: Option<String>,
    pub parcel: Option<String>,
    #[serde(rename = "realmName")]
    pub realm_name: Option<String>,
}

pub fn place_from_metadata(meta: &Value) -> Option<String> {
    let realm_name = meta_str(meta, "realmName")
        .or_else(|| meta.get("realm").and_then(|r| meta_str(r, "serverName")));
    if let Some(realm) = &realm_name {
        if realm.ends_with(".eth") {
            return Some(realm.clone());
        }
    }
    meta_str(meta, "sceneId")
}

pub(crate) fn meta_str(meta: &Value, key: &str) -> Option<String> {
    meta.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn scene_id_from_urn(urn: &str) -> Option<String> {
    let rest = urn.strip_prefix("urn:decentraland:entity:")?;
    let hash = rest.split('?').next().unwrap_or(rest);
    if hash.is_empty() {
        None
    } else {
        Some(hash.to_string())
    }
}

fn parse_scene_id_from_about(about: &Value) -> Option<String> {
    let scenes_urn = about
        .get("configurations")
        .and_then(|c| c.get("scenesUrn"))
        .and_then(|s| s.as_array())?;
    let first = scenes_urn.first().and_then(|v| v.as_str())?;
    scene_id_from_urn(first)
}

pub async fn fetch_world_scene_id(state: &AppState, world_name: &str) -> Option<String> {
    let url = format!(
        "{}/world/{}/about",
        state.world_content_url,
        crate::http::encode_path_segment(&world_name.to_lowercase())
    );
    let resp = state.http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        tracing::warn!(world = %world_name, status = %resp.status(), "world /about fetch returned non-2xx");
        return None;
    }
    let about: Value = resp.json().await.ok()?;
    parse_scene_id_from_about(&about)
}

pub async fn fetch_world_scene_id_by_pointer(
    state: &AppState,
    world_name: &str,
    pointer: &str,
) -> Option<String> {
    let url = format!(
        "{}/world/{}/scenes",
        state.world_content_url,
        crate::http::encode_path_segment(&world_name.to_lowercase())
    );
    let resp = state
        .http
        .post(&url)
        .json(&serde_json::json!({ "pointers": [pointer] }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!(world = %world_name, %pointer, status = %resp.status(), "world /scenes fetch returned non-2xx");
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    body.get("scenes")
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.get("entityId"))
        .and_then(|id| id.as_str())
        .filter(|id| !id.is_empty())
        .map(String::from)
}

pub async fn get_scene_adapter(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sf = verify_signed_fetch(&headers, "post", "/get-scene-adapter", &[])
        .map_err(|e| auth_error(e.status, e.message))?;
    let body: SceneAdapterRequest = serde_json::from_slice(&raw_body).unwrap_or_default();

    let identity = sf.signer.to_lowercase();

    let realm_name = meta_str(&sf.metadata, "realmName")
        .or_else(|| {
            sf.metadata
                .get("realm")
                .and_then(|r| meta_str(r, "serverName"))
        })
        .or(body.realm_name.clone())
        .ok_or_else(|| unauthorized("Access denied, invalid signed-fetch request, no realmName"))?;
    let scene_id = meta_str(&sf.metadata, "sceneId")
        .or(body.scene_id.clone())
        .ok_or_else(|| {
            ApiError::bad_request("Access denied, invalid signed-fetch request, no sceneId")
        })?;
    let scene_id = scene_id.as_str();
    let realm_name = realm_name.as_str();
    let is_world = realm_name.ends_with(".eth");

    let resolved_scene_id: String = if is_world && scene_id.ends_with(".eth") {
        match fetch_world_scene_id(&state, realm_name).await {
            Some(id) => id,
            None => {
                tracing::error!(world = %realm_name, "failed to resolve scene ID for world");
                return Err(ApiError::bad_request(format!(
                    "Failed to resolve scene ID for world {realm_name}"
                )));
            }
        }
    } else {
        scene_id.to_string()
    };

    let ip_address = get_request_ip(&headers);
    let device_id = device_identifier(&sf.metadata);
    if let Err(e) = state
        .player_connection
        .upsert(UpsertPlayerConnection {
            address: identity.clone(),
            ip_address,
            device_id: device_id.clone(),
        })
        .await
    {
        tracing::warn!(error = %e, address = %identity, "failed to store player connection info");
    }

    let (user_banned, scene_banned) = tokio::try_join!(
        state
            .user_bans
            .is_banned_for_connection(&identity, device_id.as_deref()),
        state.scene_bans.is_banned(&resolved_scene_id, &identity),
    )?;
    if user_banned {
        return Err(forbidden("Access denied, platform-banned user"));
    }
    if scene_banned {
        return Err(forbidden("User is banned from this scene"));
    }

    let room = if is_world {
        world_scene_room_name(realm_name, resolved_scene_id.as_str())
    } else {
        scene_room_name(resolved_scene_id.as_str())
    };

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &identity,
        VideoGrants::join(&room),
    )
    .with_ttl(Duration::from_secs(state.livekit_token_ttl_secs))
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let adapter = build_adapter_url(&state.livekit_ws_url, &token);

    Ok(Json(serde_json::json!({
        "adapter": adapter,
    })))
}

pub async fn get_server_scene_adapter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SceneAdapterRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = try_extract_signer(&headers, "post", "/get-server-scene-adapter");
    let identity = signer
        .or(body.identity.clone())
        .ok_or_else(|| unauthorized("missing identity (no auth chain, no body.identity)"))?
        .to_lowercase();

    match state.authoritative_server_address.as_deref() {
        Some(expected) if identity == expected.to_lowercase() => {}
        _ => return Err(unauthorized("Access denied, invalid server public key")),
    }

    let scene_id = body
        .scene_id
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("missing sceneId"))?;
    let realm_name = body.realm_name.as_deref().unwrap_or("main");
    let is_world = realm_name.ends_with(".eth");

    let room = if is_world {
        world_scene_room_name(realm_name, scene_id)
    } else {
        scene_room_name(scene_id)
    };

    const AUTH_SERVER_IDENTITY: &str = "authoritative-server";
    let mut grants = VideoGrants::join(&room);
    grants.can_publish = true;
    grants.can_subscribe = true;

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        AUTH_SERVER_IDENTITY,
        grants,
    )
    .with_ttl(Duration::from_secs(state.livekit_token_ttl_secs))
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let adapter = build_adapter_url(&state.livekit_ws_url, &token);

    Ok(Json(serde_json::json!({
        "adapter": adapter,
    })))
}

#[cfg(test)]
mod tests {
    use super::{parse_scene_id_from_about, scene_id_from_urn};
    use serde_json::json;

    #[test]
    fn scene_id_from_urn_strips_prefix_and_query() {
        assert_eq!(
            scene_id_from_urn(
                "urn:decentraland:entity:bafkreiabcdef123?baseUrl=https://x/contents/"
            ),
            Some("bafkreiabcdef123".to_string())
        );
        assert_eq!(
            scene_id_from_urn("urn:decentraland:entity:bafybeigdyrzt"),
            Some("bafybeigdyrzt".to_string())
        );
    }

    #[test]
    fn scene_id_from_urn_rejects_bad_input() {
        assert_eq!(scene_id_from_urn("not-a-urn"), None);
        assert_eq!(scene_id_from_urn("urn:decentraland:entity:"), None);
        assert_eq!(scene_id_from_urn("urn:decentraland:entity:?x=1"), None);
    }

    #[test]
    fn parse_scene_id_from_about_reads_first_scenes_urn() {
        let about = json!({
            "configurations": {
                "scenesUrn": [
                    "urn:decentraland:entity:bafkreiabcdef123?baseUrl=https://x/contents/",
                    "urn:decentraland:entity:bafkreiother?baseUrl=https://x/contents/"
                ]
            }
        });
        assert_eq!(
            parse_scene_id_from_about(&about),
            Some("bafkreiabcdef123".to_string())
        );
    }

    #[test]
    fn parse_scene_id_from_about_none_when_no_scenes() {
        assert_eq!(parse_scene_id_from_about(&json!({})), None);
        assert_eq!(
            parse_scene_id_from_about(&json!({ "configurations": { "scenesUrn": [] } })),
            None
        );
        assert_eq!(
            parse_scene_id_from_about(
                &json!({ "configurations": { "scenesUrn": ["urn:decentraland:collection:foo"] } })
            ),
            None
        );
    }
}
