use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth_chain::{try_extract_signer, verify_signed_fetch};
use crate::http::{auth_error, forbidden, unauthorized, ApiError};
use crate::livekit::{build_adapter_url, scene_room_name, world_scene_room_name, AccessToken, VideoGrants};
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
    let realm_name = meta_str(meta, "realmName").or_else(|| {
        meta.get("realm").and_then(|r| meta_str(r, "serverName"))
    });
    if let Some(realm) = &realm_name {
        if realm.ends_with(".eth") {
            return Some(realm.clone());
        }
    }
    meta_str(meta, "sceneId")
}

fn meta_str(meta: &Value, key: &str) -> Option<String> {
    meta.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
        .ok_or_else(|| {
            unauthorized("Access denied, invalid signed-fetch request, no realmName")
        })?;
    let scene_id = meta_str(&sf.metadata, "sceneId")
        .or(body.scene_id.clone())
        .ok_or_else(|| {
            ApiError::bad_request("Access denied, invalid signed-fetch request, no sceneId")
        })?;
    let scene_id = scene_id.as_str();
    let realm_name = realm_name.as_str();
    let is_world = realm_name.ends_with(".eth");

    let place_id = if is_world {
        realm_name.to_string()
    } else {
        scene_id.to_string()
    };

    let (user_banned, scene_banned) = tokio::try_join!(
        state.user_bans.is_banned(&identity),
        state.scene_bans.is_banned(&place_id, &identity),
    )?;
    if user_banned {
        return Err(forbidden("Access denied, platform-banned user"));
    }
    if scene_banned {
        return Err(forbidden("User is banned from this scene"));
    }

    let room = if is_world {
        world_scene_room_name(realm_name, scene_id)
    } else {
        scene_room_name(realm_name, scene_id)
    };

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &identity,
        VideoGrants::join(&room),
    )
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
        scene_room_name(realm_name, scene_id)
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
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let adapter = build_adapter_url(&state.livekit_ws_url, &token);

    Ok(Json(serde_json::json!({
        "adapter": adapter,
    })))
}
