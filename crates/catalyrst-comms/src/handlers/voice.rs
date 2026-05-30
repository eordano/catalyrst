use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::try_extract_signer;
use crate::http::{service_unavailable, unauthorized, ApiError};
use crate::livekit::{
    build_adapter_url, community_voice_chat_room_name, private_voice_chat_room_name, AccessToken,
    VideoGrants,
};
use crate::AppState;

const PRESENCE_TTL_SECS: i64 = 300;

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
    addr.len() == 42
        && addr.starts_with("0x")
        && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
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

    let identity = try_extract_signer(&headers, "get", "/private-messages/token")
        .ok_or_else(|| unauthorized("Access denied, invalid identity"))?
        .to_lowercase();

    let banned = state.user_bans.is_banned(&identity).await?;
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
        return Err(ApiError::bad_request("Invalid request body, missing room_id"));
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

    let room_name = private_voice_chat_room_name(&body.room_id);

    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for addr in &addresses {
        let mut grants = VideoGrants::join(&room_name);
        grants.can_publish = true;
        grants.can_subscribe = true;
        grants.can_update_own_metadata = false;

        let token = AccessToken::new(
            &state.livekit_api_key,
            &state.livekit_api_secret,
            addr,
            grants,
        )
        .to_jwt()
        .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;
        let connection_url = build_adapter_url(&state.livekit_ws_url, &token);
        out.insert(
            addr.clone(),
            serde_json::json!({ "connection_url": connection_url }),
        );

        sqlx::query(
            "INSERT INTO voice_chat_users (address, room_name, status, joined_at, status_updated_at) \
             VALUES ($1, $2, 'connected', now(), now()) \
             ON CONFLICT (address, room_name) \
             DO UPDATE SET status = 'connected', status_updated_at = now()",
        )
        .bind(addr)
        .bind(&room_name)
        .execute(&state.pool)
        .await?;
    }

    Ok(Json(serde_json::to_value(out).unwrap_or(serde_json::json!({}))))
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
        return Err(ApiError::bad_request("Invalid request body, invalid address"));
    }

    let room_name = private_voice_chat_room_name(&id);

    let users_in_room: Vec<String> =
        sqlx::query_scalar::<_, String>("SELECT address FROM voice_chat_users WHERE room_name = $1")
            .bind(&room_name)
            .fetch_all(&state.pool)
            .await?;

    if users_in_room.is_empty() {
        return Err(ApiError::not_found(format!("Room {id} does not exist")));
    }

    sqlx::query("DELETE FROM voice_chat_users WHERE room_name = $1")
        .bind(&room_name)
        .execute(&state.pool)
        .await?;

    if let Err(e) = state.room_service().delete_room(&room_name).await {
        tracing::warn!(error = %e, room = %room_name, "failed to delete livekit private voice room");
    }

    Ok(Json(
        serde_json::json!({ "users_in_voice_chat": users_in_room }),
    ))
}

pub async fn get_voice_chat_status(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = address.to_lowercase();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM voice_chat_users \
         WHERE address = $1 AND status = 'connected' \
           AND status_updated_at > now() - ($2 || ' seconds')::interval",
    )
    .bind(&address)
    .bind(PRESENCE_TTL_SECS.to_string())
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "is_user_in_voice_chat": count > 0
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

pub async fn community_voice_chat_create_or_join(
    State(state): State<AppState>,
    Json(body): Json<CommunityVoiceChatBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;

    if body.community_id.trim().is_empty() {
        return Err(ApiError::bad_request("The property community_id is required"));
    }
    let user_address = body.user_address.to_lowercase();
    if !is_eth_address(&user_address) {
        return Err(ApiError::bad_request("The property user_address is invalid"));
    }
    let role = body.user_role.as_deref().unwrap_or("none").to_lowercase();
    let action = body.action.as_deref().unwrap_or("join").to_lowercase();
    let is_creating = action == "create";
    let is_speaker = is_creating && is_moderator_role(&role);

    let room_name = community_voice_chat_room_name(&body.community_id);

    let mut metadata = serde_json::Map::new();
    metadata.insert("role".into(), serde_json::json!(role));
    metadata.insert("isSpeaker".into(), serde_json::json!(is_speaker));
    metadata.insert("muted".into(), serde_json::json!(false));
    if let Some(profile) = body.profile_data.as_ref().and_then(|v| v.as_object()) {
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

    sqlx::query(
        "INSERT INTO community_voice_chat_users \
         (address, room_name, is_moderator, status, joined_at, status_updated_at, created_at) \
         VALUES ($1, $2, $3, 'connected', now(), now(), now()) \
         ON CONFLICT (address, room_name) \
         DO UPDATE SET is_moderator = $3, status = 'connected', status_updated_at = now()",
    )
    .bind(&user_address)
    .bind(&room_name)
    .bind(is_moderator_role(&role))
    .execute(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({ "connection_url": connection_url })))
}

async fn community_status(state: &AppState, community_id: &str) -> Result<(bool, i64, i64), ApiError> {
    let room_name = community_voice_chat_room_name(community_id);
    let row: (i64, i64) = sqlx::query_as(
        "SELECT \
            count(*) FILTER (WHERE status = 'connected' AND status_updated_at > now() - ($2 || ' seconds')::interval), \
            count(*) FILTER (WHERE is_moderator AND status = 'connected' AND status_updated_at > now() - ($2 || ' seconds')::interval) \
         FROM community_voice_chat_users WHERE room_name = $1",
    )
    .bind(&room_name)
    .bind(PRESENCE_TTL_SECS.to_string())
    .fetch_one(&state.pool)
    .await?;
    let (participant_count, moderator_count) = row;
    let active = moderator_count > 0;
    Ok((active, participant_count, moderator_count))
}

pub async fn community_voice_chat_status(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("The parameter communityId is required"));
    }
    let (active, participant_count, moderator_count) =
        community_status(&state, &community_id).await?;
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
    let rooms: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT room_name FROM community_voice_chat_users \
         WHERE is_moderator AND status = 'connected' \
           AND status_updated_at > now() - ($1 || ' seconds')::interval",
    )
    .bind(PRESENCE_TTL_SECS.to_string())
    .fetch_all(&state.pool)
    .await?;

    let prefix = format!("{}-", crate::livekit::COMMUNITY_VOICE_CHAT_ROOM_PREFIX);
    let mut data = Vec::with_capacity(rooms.len());
    for room in rooms {
        let community_id = room.strip_prefix(&prefix).unwrap_or(&room).to_string();
        let (_, participant_count, moderator_count) =
            community_status(&state, &community_id).await?;
        data.push(serde_json::json!({
            "communityId": community_id,
            "participantCount": participant_count,
            "moderatorCount": moderator_count,
        }));
    }

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
        return Err(ApiError::bad_request("The parameter communityId is required"));
    }
    if body.user_address.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(ApiError::bad_request("The property user_address is required"));
    }

    let room_name = community_voice_chat_room_name(&community_id);

    sqlx::query("DELETE FROM community_voice_chat_users WHERE room_name = $1")
        .bind(&room_name)
        .execute(&state.pool)
        .await?;

    if let Err(e) = state.room_service().delete_room(&room_name).await {
        tracing::warn!(error = %e, room = %room_name, "failed to delete livekit community voice room");
    }

    Ok(Json(serde_json::json!({
        "message": "Community voice chat ended successfully"
    })))
}

// --- Per-user community voice-chat participant actions -----------------------
//
// These mirror upstream comms-gatekeeper's community-voice-chat sub-routes
// (`/community-voice-chat/:communityId/users/:userAddress/...`). Each one
// mutates LiveKit participant state via the RoomService `UpdateParticipant`
// call: speak-request / mute flip metadata, promote/demote flip both the
// publish permission and metadata, kick removes the participant.

fn require_community_and_user(
    community_id: &str,
    user_address: &str,
) -> Result<String, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("The parameter communityId is required"));
    }
    let addr = user_address.to_lowercase();
    if addr.trim().is_empty() {
        return Err(ApiError::bad_request("The parameter userAddress is required"));
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

/// POST .../speak-request — raise hand (request to speak).
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

/// DELETE .../speak-request — lower hand / reject a pending speak request.
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

/// POST .../speaker — promote a listener to speaker (grant publish).
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

/// DELETE .../speaker — demote a speaker to listener (revoke publish).
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

/// DELETE .../users/:userAddress — kick a participant out of the room.
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

/// PATCH .../mute — mute / unmute a speaker (metadata flag only; the client
/// enforces the mute, matching upstream behaviour).
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
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM community_voice_chat_users \
         WHERE address = $1 AND status = 'connected' \
           AND status_updated_at > now() - ($2 || ' seconds')::interval",
    )
    .bind(&user_address)
    .bind(PRESENCE_TTL_SECS.to_string())
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "userAddress": user_address,
        "isInCommunityVoiceChat": count > 0,
    })))
}
