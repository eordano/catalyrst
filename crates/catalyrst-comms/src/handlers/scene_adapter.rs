use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth_chain::{require_signer, verify_signed_fetch};
use crate::extract::{device_identifier, get_request_ip};
use crate::handlers::responses::SceneAdapterResponse;
use crate::http::{auth_error, forbidden, unauthorized, ApiError};
use crate::livekit::{
    build_adapter_url, is_world_realm_name, join_grants, scene_room_name, world_scene_room_name,
    AccessToken,
};
use crate::ports::extra_addresses::has_world_access_permission;
use crate::ports::player_connection::UpsertPlayerConnection;
use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct SceneAdapterRequest {
    #[serde(rename = "sceneId")]
    pub scene_id: Option<String>,
    pub parcel: Option<String>,
    #[serde(rename = "realmName")]
    pub realm_name: Option<String>,
}

pub fn place_from_metadata(meta: &Value) -> Option<String> {
    if let Some(realm) = realm_name_from_metadata(meta) {
        if is_world_realm_name(&realm) {
            return Some(realm);
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
    state
        .world_permissions
        .about_scene_ids
        .get_or_fetch(url.clone(), || async {
            let resp = state.http.get(&url).send().await.map_err(|_| ())?;
            if !resp.status().is_success() {
                tracing::warn!(world = %world_name, status = %resp.status(), "world /about fetch returned non-2xx");
                return Err(());
            }
            let about: Value = resp.json().await.map_err(|_| ())?;
            parse_scene_id_from_about(&about).ok_or(())
        })
        .await
        .ok()
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

pub fn adapter_room_name(realm_name: &str, scene_id: &str) -> String {
    if is_world_realm_name(realm_name) {
        world_scene_room_name(realm_name, scene_id)
    } else {
        scene_room_name(realm_name, scene_id)
    }
}

pub fn realm_name_from_metadata(metadata: &Value) -> Option<String> {
    meta_str(metadata, "realmName").or_else(|| {
        metadata
            .get("realm")
            .and_then(|r| meta_str(r, "serverName"))
    })
}

pub fn realm_name_from(metadata: &Value, body: &SceneAdapterRequest) -> Option<String> {
    realm_name_from_metadata(metadata).or_else(|| body.realm_name.clone())
}

pub fn scene_id_from(metadata: &Value, body: &SceneAdapterRequest) -> Option<String> {
    meta_str(metadata, "sceneId").or_else(|| body.scene_id.clone())
}

async fn resolve_world_scene_id(
    state: &AppState,
    realm_name: &str,
    scene_id: &str,
) -> Result<String, ApiError> {
    if !is_world_realm_name(realm_name) || !is_world_realm_name(scene_id) {
        return Ok(scene_id.to_string());
    }
    match fetch_world_scene_id(state, realm_name).await {
        Some(id) => Ok(id),
        None => {
            tracing::error!(world = %realm_name, "failed to resolve scene ID for world");
            Err(ApiError::bad_request(format!(
                "Failed to resolve scene ID for world {realm_name}"
            )))
        }
    }
}

pub async fn get_scene_adapter(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<SceneAdapterResponse>, ApiError> {
    let sf = verify_signed_fetch(&headers, "post", "/get-scene-adapter", &[])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let body: SceneAdapterRequest = serde_json::from_slice(&raw_body).unwrap_or_default();

    let identity = sf.signer.as_str().to_string();

    let realm_name = realm_name_from(&sf.metadata, &body)
        .ok_or_else(|| unauthorized("Access denied, invalid signed-fetch request, no realmName"))?;
    let scene_id = scene_id_from(&sf.metadata, &body).ok_or_else(|| {
        ApiError::bad_request("Access denied, invalid signed-fetch request, no sceneId")
    })?;
    let scene_id = scene_id.as_str();
    let realm_name = realm_name.as_str();
    let is_world = is_world_realm_name(realm_name);

    let resolved_scene_id = resolve_world_scene_id(&state, realm_name, scene_id).await?;

    let ip_address = get_request_ip(&headers);
    let device_id = device_identifier(&sf.metadata);
    let upsert = async {
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
    };
    let world_access =
        async { !is_world || has_world_access_permission(&state, &identity, realm_name).await };
    let ((), gate, world_allowed) = tokio::join!(
        upsert,
        state
            .user_bans
            .connection_gate(&identity, device_id.as_deref(), &resolved_scene_id),
        world_access,
    );
    let (user_banned, scene_banned) = gate?;
    if user_banned {
        return Err(forbidden(crate::access_gate::PLATFORM_BANNED_MSG));
    }
    if scene_banned {
        return Err(forbidden("User is banned from this scene"));
    }
    if !world_allowed {
        return Err(unauthorized(
            "Access denied, you are not authorized to access this world",
        ));
    }

    let room = adapter_room_name(realm_name, resolved_scene_id.as_str());

    let mut grants = join_grants(&room);
    grants.can_update_own_metadata = false;

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &identity,
        grants,
    )
    .with_metadata(serde_json::json!({ "isGuest": sf.is_guest }).to_string())
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let adapter = build_adapter_url(&state.livekit_ws_url, &token);

    Ok(Json(SceneAdapterResponse { adapter }))
}

pub async fn get_server_scene_adapter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SceneAdapterRequest>,
) -> Result<Json<SceneAdapterResponse>, ApiError> {
    let identity = require_signer(&headers, "post", "/get-server-scene-adapter")
        .await
        .map_err(|e| unauthorized(format!("Access denied, invalid signed-fetch request: {e}")))?
        .as_str()
        .to_string();

    match state.authoritative_server_address.as_deref() {
        Some(expected) if identity == expected.to_lowercase() => {}
        _ => return Err(unauthorized("Access denied, invalid server public key")),
    }

    let realm_name = body
        .realm_name
        .as_deref()
        .ok_or_else(|| unauthorized("Access denied, invalid signed-fetch request, no realm"))?;
    let scene_id = body.scene_id.as_deref().ok_or_else(|| {
        ApiError::bad_request("Access denied, invalid signed-fetch request, no sceneId")
    })?;
    let resolved_scene_id = resolve_world_scene_id(&state, realm_name, scene_id).await?;
    let room = adapter_room_name(realm_name, resolved_scene_id.as_str());

    const AUTH_SERVER_IDENTITY: &str = "authoritative-server";
    let mut grants = join_grants(&room);
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

    Ok(Json(SceneAdapterResponse { adapter }))
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
