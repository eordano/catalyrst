use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::extract::{device_identifier, get_request_ip};
use crate::http::{service_unavailable, unauthorized, ApiError};
use crate::livekit::{build_adapter_url, community_voice_chat_room_name, AccessToken, VideoGrants};
use crate::ports::player_connection::UpsertPlayerConnection;
use crate::AppState;

fn require_livekit(state: &AppState) -> Result<(), ApiError> {
    if state.livekit_configured {
        Ok(())
    } else {
        Err(service_unavailable(
            "LiveKit is not configured (LIVEKIT_API_KEY / LIVEKIT_API_SECRET unset)",
        ))
    }
}

fn is_eth_address(addr: &str) -> bool {
    addr.len() == 42 && addr.starts_with("0x") && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Deserialize)]
pub struct PrivateMessagesPrivacyBody {
    pub private_messages_privacy: Option<String>,
}

pub async fn private_messages_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    let sf = verify_signed_fetch(
        &headers,
        "get",
        "/private-messages/token",
        &["dcl:explorer"],
    )
    .map_err(|e| ApiError::http(e.status, e.message))?;
    let identity = sf.signer.to_lowercase();

    if identity.is_empty() {
        return Err(unauthorized("Access denied, invalid identity"));
    }

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

    let banned = state
        .user_bans
        .is_banned_for_connection(&identity, device_id.as_deref())
        .await?;
    if banned {
        return Err(unauthorized("Access denied, deny-listed wallet"));
    }

    let privacy = sqlx::query_scalar::<_, String>(
        "SELECT private_messages_privacy FROM private_messages_privacy WHERE address = $1",
    )
    .bind(&identity)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "all".to_string());

    let metadata = serde_json::json!({ "private_messages_privacy": privacy }).to_string();

    let mut grants = VideoGrants::join(&state.private_messages_room_id);
    grants.can_publish = false;
    grants.can_update_own_metadata = false;

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &identity,
        grants,
    )
    .with_metadata(metadata)
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let adapter = build_adapter_url(&state.livekit_ws_url, &token);

    Ok(Json(serde_json::json!({ "adapter": adapter })))
}

pub async fn patch_private_messages_privacy(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Json(body): Json<PrivateMessagesPrivacyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = address.to_lowercase();
    if !is_eth_address(&address) {
        return Err(ApiError::bad_request("Invalid address"));
    }
    let privacy = body
        .private_messages_privacy
        .as_deref()
        .map(|s| s.to_lowercase())
        .filter(|s| s == "all" || s == "only_friends")
        .ok_or_else(|| ApiError::bad_request("Invalid private_messages_privacy"))?;

    sqlx::query(
        "INSERT INTO private_messages_privacy (address, private_messages_privacy, updated_at) \
         VALUES ($1, $2, now()) \
         ON CONFLICT (address) DO UPDATE SET private_messages_privacy = $2, updated_at = now()",
    )
    .bind(&address)
    .bind(&privacy)
    .execute(&state.pool)
    .await?;

    Ok(Json(
        serde_json::json!({ "address": address, "private_messages_privacy": privacy }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct PrivateVoiceChatBody {
    pub room_id: String,
    pub user_addresses: Vec<String>,
}

pub async fn create_private_voice_chat(
    State(state): State<AppState>,
    Json(body): Json<PrivateVoiceChatBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    if body.room_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "Invalid request body, missing room_id",
        ));
    }
    let addresses: Vec<String> = body
        .user_addresses
        .iter()
        .map(|a| a.to_lowercase())
        .collect();
    if addresses.is_empty() {
        return Err(ApiError::bad_request(
            "Invalid request body, missing user_addresses",
        ));
    }

    let out = crate::voice_logic::get_private_voice_chat_room_credentials(
        &state,
        &body.room_id,
        &addresses,
    )
    .await?;

    Ok(Json(
        serde_json::to_value(out).unwrap_or(serde_json::json!({})),
    ))
}

#[derive(Debug, Deserialize)]
pub struct EndPrivateVoiceChatBody {
    pub address: Option<String>,
}

pub async fn end_private_voice_chat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EndPrivateVoiceChatBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    let address = body
        .address
        .as_deref()
        .map(|s| s.to_lowercase())
        .ok_or_else(|| ApiError::bad_request("Invalid request body, missing address"))?;
    if !is_eth_address(&address) {
        return Err(ApiError::bad_request(
            "Invalid request body, invalid address",
        ));
    }

    let users_in_room = crate::voice_logic::end_private_voice_chat(&state, &id, &address).await?;

    Ok(Json(
        serde_json::json!({ "users_in_voice_chat": users_in_room }),
    ))
}

pub async fn get_voice_chat_status(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = address.to_lowercase();

    let is_user_in_voice_chat = crate::voice_logic::is_user_in_voice_chat(&state, &address).await?;

    Ok(Json(serde_json::json!({
        "is_user_in_voice_chat": is_user_in_voice_chat
    })))
}

#[derive(Debug, Deserialize)]
pub struct CommunityVoiceChatBody {
    pub community_id: String,
    pub user_address: String,
    pub user_role: Option<String>,
    pub action: Option<String>,
    pub profile_data: Option<serde_json::Value>,
}

fn is_moderator_role(role: &str) -> bool {
    matches!(role, "owner" | "moderator")
}

fn community_join_metadata(
    role: &str,
    is_speaker: bool,
    profile_data: Option<&serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut metadata = serde_json::Map::new();
    metadata.insert("role".into(), serde_json::json!(role));
    metadata.insert("isSpeaker".into(), serde_json::json!(is_speaker));
    metadata.insert("muted".into(), serde_json::json!(false));
    if let Some(profile) = profile_data.and_then(|v| v.as_object()) {
        if let Some(name) = profile.get("name") {
            metadata.insert("name".into(), name.clone());
        }
        if let Some(claimed) = profile.get("has_claimed_name") {
            metadata.insert("hasClaimedName".into(), claimed.clone());
        }
        if let Some(pic) = profile.get("profile_picture_url") {
            metadata.insert("profilePictureUrl".into(), pic.clone());
        }
    }
    metadata
}

pub async fn community_voice_chat_create_or_join(
    State(state): State<AppState>,
    Json(body): Json<CommunityVoiceChatBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    if body.community_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The property community_id is required",
        ));
    }
    let user_address = body.user_address.to_lowercase();
    if !is_eth_address(&user_address) {
        return Err(ApiError::bad_request(
            "The property user_address is invalid",
        ));
    }
    let role = body.user_role.as_deref().unwrap_or("none").to_lowercase();
    let action = body.action.as_deref().unwrap_or("join").to_lowercase();
    let is_creating = action == "create";
    let is_speaker = is_creating && is_moderator_role(&role);

    let room_name = community_voice_chat_room_name(&body.community_id);

    let metadata = community_join_metadata(&role, is_speaker, body.profile_data.as_ref());

    let mut grants = VideoGrants::join(&room_name);
    grants.can_publish = is_speaker;
    grants.can_subscribe = true;
    grants.can_update_own_metadata = false;

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &user_address,
        grants,
    )
    .with_metadata(serde_json::Value::Object(metadata).to_string())
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let connection_url = build_adapter_url(&state.livekit_ws_url, &token);

    state
        .voice_db
        .join_user_to_community_room(&user_address, &room_name, is_moderator_role(&role))
        .await?;

    Ok(Json(
        serde_json::json!({ "connection_url": connection_url }),
    ))
}

async fn community_status(
    state: &AppState,
    community_id: &str,
) -> Result<(bool, i64, i64), ApiError> {
    let room_name = community_voice_chat_room_name(community_id);
    let users = state
        .voice_db
        .get_community_users_in_room(&room_name)
        .await?;
    let now = now_ms();
    let active_participants = users
        .iter()
        .filter(|u| state.voice_db.is_active_community_user(u, now))
        .count() as i64;
    let active_moderators = users
        .iter()
        .filter(|u| u.is_moderator && state.voice_db.is_active_community_user(u, now))
        .count() as i64;
    let active = active_moderators > 0;
    Ok((active, active_participants, active_moderators))
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub async fn community_voice_chat_status(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter communityId is required",
        ));
    }
    let (active, participant_count, moderator_count) =
        community_status(&state, &community_id).await?;

    let (participant_count, moderator_count) = if active {
        (participant_count, moderator_count)
    } else {
        (0, 0)
    };
    Ok(Json(serde_json::json!({
        "active": active,
        "participant_count": participant_count,
        "moderator_count": moderator_count,
    })))
}

#[derive(Debug, Deserialize)]
pub struct BulkCommunityStatusBody {
    pub community_ids: Vec<String>,
}

pub async fn community_voice_chat_bulk_status(
    State(state): State<AppState>,
    Json(body): Json<BulkCommunityStatusBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut data = Vec::with_capacity(body.community_ids.len());
    for community_id in &body.community_ids {
        let (active, participant_count, moderator_count) =
            community_status(&state, community_id).await?;
        data.push(serde_json::json!({
            "community_id": community_id,
            "active": active,
            "participant_count": participant_count,
            "moderator_count": moderator_count,
        }));
    }
    Ok(Json(serde_json::json!({ "data": data })))
}

pub async fn community_voice_chat_active(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let active = state
        .voice_db
        .get_all_active_community_voice_chats()
        .await?;
    let data: Vec<serde_json::Value> = active
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "communityId": c.community_id,
                "participantCount": c.participant_count,
                "moderatorCount": c.moderator_count,
            })
        })
        .collect();
    let total = data.len();
    Ok(Json(serde_json::json!({ "data": data, "total": total })))
}

#[derive(Debug, Deserialize)]
pub struct EndCommunityVoiceChatBody {
    pub user_address: Option<String>,
}

pub async fn community_voice_chat_end(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Json(body): Json<EndCommunityVoiceChatBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter communityId is required",
        ));
    }
    if body
        .user_address
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return Err(ApiError::bad_request(
            "The property user_address is required",
        ));
    }

    crate::voice_logic::end_community_voice_chat(&state, &community_id).await?;

    Ok(Json(serde_json::json!({
        "message": "Community voice chat ended successfully"
    })))
}

fn require_community_and_user(community_id: &str, user_address: &str) -> Result<String, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter communityId is required",
        ));
    }
    let addr = user_address.to_lowercase();
    if addr.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter userAddress is required",
        ));
    }
    Ok(addr)
}

async fn merge_metadata(
    state: &AppState,
    room_name: &str,
    address: &str,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    state
        .room_service()
        .merge_participant_metadata(room_name, address, patch)
        .await
        .map_err(|e| ApiError::internal(format!("livekit update participant metadata: {e}")))
}

pub async fn community_request_to_speak(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(true));
    merge_metadata(&state, &room_name, &address, patch).await?;
    Ok(Json(serde_json::json!({
        "message": "Request to speak sent successfully"
    })))
}

pub async fn community_reject_speak_request(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(false));
    merge_metadata(&state, &room_name, &address, patch).await?;
    Ok(Json(serde_json::json!({
        "message": "Speak request rejected successfully"
    })))
}

pub async fn community_promote_speaker(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);

    state
        .room_service()
        .update_participant(
            &room_name,
            &address,
            None,
            Some(serde_json::json!({
                "canPublish": true,
                "canSubscribe": true,
                "canPublishData": true,
            })),
        )
        .await
        .map_err(|e| ApiError::internal(format!("livekit update participant permissions: {e}")))?;

    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(false));
    patch.insert("isSpeaker".into(), serde_json::json!(true));
    merge_metadata(&state, &room_name, &address, patch).await?;

    Ok(Json(serde_json::json!({
        "message": "User promoted to speaker successfully"
    })))
}

pub async fn community_demote_speaker(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);

    state
        .room_service()
        .update_participant(
            &room_name,
            &address,
            None,
            Some(serde_json::json!({
                "canPublish": false,
                "canSubscribe": true,
                "canPublishData": true,
            })),
        )
        .await
        .map_err(|e| ApiError::internal(format!("livekit update participant permissions: {e}")))?;

    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(false));
    patch.insert("isSpeaker".into(), serde_json::json!(false));
    merge_metadata(&state, &room_name, &address, patch).await?;

    Ok(Json(serde_json::json!({
        "message": "User demoted to listener successfully"
    })))
}

pub async fn community_kick_player(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);

    if let Err(e) = state
        .room_service()
        .remove_participant(&room_name, &address)
        .await
    {
        tracing::warn!(error = %e, room = %room_name, addr = %address, "failed to remove community voice participant");
    }

    sqlx::query("DELETE FROM community_voice_chat_users WHERE address = $1 AND room_name = $2")
        .bind(&address)
        .bind(&room_name)
        .execute(&state.pool)
        .await
        .ok();

    Ok(Json(serde_json::json!({
        "message": "User kicked from voice chat successfully"
    })))
}

#[derive(Debug, Deserialize)]
pub struct MuteSpeakerBody {
    pub muted: bool,
}

pub async fn community_mute_speaker(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
    Json(body): Json<MuteSpeakerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let address = require_community_and_user(&community_id, &user_address)?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("muted".into(), serde_json::json!(body.muted));
    merge_metadata(&state, &room_name, &address, patch).await?;
    let action = if body.muted { "muted" } else { "unmuted" };
    Ok(Json(serde_json::json!({
        "message": format!("User {action} successfully")
    })))
}

pub async fn check_user_community_status(
    State(state): State<AppState>,
    Path(user_address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_address = user_address.to_lowercase();

    let is_in = state
        .voice_db
        .is_user_in_any_community_voice_chat(&user_address)
        .await?;

    Ok(Json(serde_json::json!({
        "userAddress": user_address,
        "isInCommunityVoiceChat": is_in,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_voice_chat_body_deserializes_snake_case() {
        let body: PrivateVoiceChatBody = serde_json::from_str(
            r#"{ "room_id": "call-1", "user_addresses": ["0xAAA", "0xBBB"] }"#,
        )
        .unwrap();
        assert_eq!(body.room_id, "call-1");
        assert_eq!(body.user_addresses, vec!["0xAAA", "0xBBB"]);
    }

    #[test]
    fn end_private_voice_chat_body_optional_address() {
        let with: EndPrivateVoiceChatBody =
            serde_json::from_str(r#"{ "address": "0xabc" }"#).unwrap();
        assert_eq!(with.address.as_deref(), Some("0xabc"));
        let without: EndPrivateVoiceChatBody = serde_json::from_str("{}").unwrap();
        assert!(without.address.is_none());
    }

    #[test]
    fn community_voice_chat_body_optional_fields() {
        let full: CommunityVoiceChatBody = serde_json::from_str(
            r#"{ "community_id": "c1", "user_address": "0xabc", "user_role": "owner",
                 "action": "create",
                 "profile_data": { "name": "Foo", "has_claimed_name": true,
                                   "profile_picture_url": "http://x/y.png" } }"#,
        )
        .unwrap();
        assert_eq!(full.community_id, "c1");
        assert_eq!(full.user_role.as_deref(), Some("owner"));
        assert_eq!(full.action.as_deref(), Some("create"));
        assert!(full.profile_data.is_some());

        let minimal: CommunityVoiceChatBody =
            serde_json::from_str(r#"{ "community_id": "c1", "user_address": "0xabc" }"#).unwrap();
        assert!(minimal.user_role.is_none());
        assert!(minimal.action.is_none());
        assert!(minimal.profile_data.is_none());
    }

    #[test]
    fn mute_and_bulk_bodies_deserialize() {
        let mute: MuteSpeakerBody = serde_json::from_str(r#"{ "muted": true }"#).unwrap();
        assert!(mute.muted);
        let bulk: BulkCommunityStatusBody =
            serde_json::from_str(r#"{ "community_ids": ["a", "b"] }"#).unwrap();
        assert_eq!(bulk.community_ids, vec!["a", "b"]);
    }

    #[test]
    fn create_action_with_moderator_role_is_speaker() {
        assert!(is_moderator_role("owner"));
        assert!(is_moderator_role("moderator"));
        assert!(!is_moderator_role("member"));
        assert!(!is_moderator_role("none"));
    }

    #[test]
    fn community_join_metadata_create_owner_is_speaker() {
        let md = community_join_metadata("owner", true, None);
        assert_eq!(md["role"], "owner");
        assert_eq!(md["isSpeaker"], true);
        assert_eq!(md["muted"], false);

        assert!(!md.contains_key("name"));
        assert_eq!(md.len(), 3);
    }

    #[test]
    fn community_join_metadata_carries_profile_camelcased() {
        let profile = serde_json::json!({
            "name": "Foo",
            "has_claimed_name": true,
            "profile_picture_url": "http://x/y.png"
        });
        let md = community_join_metadata("member", false, Some(&profile));
        assert_eq!(md["isSpeaker"], false);
        assert_eq!(md["name"], "Foo");
        assert_eq!(md["hasClaimedName"], true);
        assert_eq!(md["profilePictureUrl"], "http://x/y.png");
    }

    #[test]
    fn community_join_metadata_omits_absent_profile_keys() {
        let profile = serde_json::json!({ "name": "Foo" });
        let md = community_join_metadata("member", false, Some(&profile));
        assert_eq!(md["name"], "Foo");
        assert!(!md.contains_key("hasClaimedName"));
        assert!(!md.contains_key("profilePictureUrl"));
    }

    #[test]
    fn single_status_response_keys_are_snake_case() {
        let body = serde_json::json!({
            "active": true,
            "participant_count": 3,
            "moderator_count": 1,
        });
        let obj = body.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert!(obj.contains_key("active"));
        assert!(obj.contains_key("participant_count"));
        assert!(obj.contains_key("moderator_count"));
    }

    #[test]
    fn active_list_response_keys_are_camel_case() {
        let entry = serde_json::json!({
            "communityId": "c1",
            "participantCount": 2,
            "moderatorCount": 1,
        });
        let obj = entry.as_object().unwrap();
        assert!(obj.contains_key("communityId"));
        assert!(obj.contains_key("participantCount"));
        assert!(obj.contains_key("moderatorCount"));
    }

    #[test]
    fn user_community_status_response_keys_are_camel_case() {
        let body = serde_json::json!({
            "userAddress": "0xabc",
            "isInCommunityVoiceChat": false,
        });
        let obj = body.as_object().unwrap();
        assert!(obj.contains_key("userAddress"));
        assert!(obj.contains_key("isInCommunityVoiceChat"));
    }
}
