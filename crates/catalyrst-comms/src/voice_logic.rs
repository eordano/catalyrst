//! Private (1:1) voice-chat state-machine logic.
//!
//! Faithful port of the private-voice-chat half of upstream comms-gatekeeper
//! `src/logic/voice/voice.ts` — the `handlePrivateParticipant{Joined,Left}`,
//! `getPrivateVoiceChatRoomCredentials`, `endPrivateVoiceChat`,
//! `expirePrivateVoiceChats` and `isUserInVoiceChat` functions. This wires the
//! LiveKit room teardown (DeleteRoom) into the DB state machine in
//! [`crate::voice_db`].

use std::collections::BTreeMap;

use crate::livekit::{
    build_adapter_url, community_id_from_room_name, private_voice_chat_room_name, AccessToken,
    VideoGrants, TRACK_SOURCE_MICROPHONE,
};
use crate::voice_db::{DeleteRoomError, VoiceChatUserStatus};
use crate::AppState;

/// LiveKit `DisconnectReason` enum. Values match the well-known LiveKit
/// protobuf (`livekit_models.proto`). The webhook delivers the reason as either
/// the numeric value or the proto3-JSON name string; [`DisconnectReason::parse`]
/// accepts both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    UnknownReason,
    ClientInitiated,
    DuplicateIdentity,
    ServerShutdown,
    ParticipantRemoved,
    RoomDeleted,
    StateMismatch,
    JoinFailure,
    Migration,
    SignalClose,
    RoomClosed,
    UserUnavailable,
    UserRejected,
    SipTrunkFailure,
    /// Any reason value we do not model explicitly. Treated as an abrupt
    /// disconnect (connection interrupted), matching upstream's `else` branch.
    Other(i64),
}

impl DisconnectReason {
    /// Numeric protobuf value.
    fn from_i64(v: i64) -> Self {
        match v {
            0 => DisconnectReason::UnknownReason,
            1 => DisconnectReason::ClientInitiated,
            2 => DisconnectReason::DuplicateIdentity,
            3 => DisconnectReason::ServerShutdown,
            4 => DisconnectReason::ParticipantRemoved,
            5 => DisconnectReason::RoomDeleted,
            6 => DisconnectReason::StateMismatch,
            7 => DisconnectReason::JoinFailure,
            8 => DisconnectReason::Migration,
            9 => DisconnectReason::SignalClose,
            10 => DisconnectReason::RoomClosed,
            11 => DisconnectReason::UserUnavailable,
            12 => DisconnectReason::UserRejected,
            13 => DisconnectReason::SipTrunkFailure,
            other => DisconnectReason::Other(other),
        }
    }

    /// proto3-JSON enum name.
    fn from_name(s: &str) -> Self {
        match s {
            "UNKNOWN_REASON" => DisconnectReason::UnknownReason,
            "CLIENT_INITIATED" => DisconnectReason::ClientInitiated,
            "DUPLICATE_IDENTITY" => DisconnectReason::DuplicateIdentity,
            "SERVER_SHUTDOWN" => DisconnectReason::ServerShutdown,
            "PARTICIPANT_REMOVED" => DisconnectReason::ParticipantRemoved,
            "ROOM_DELETED" => DisconnectReason::RoomDeleted,
            "STATE_MISMATCH" => DisconnectReason::StateMismatch,
            "JOIN_FAILURE" => DisconnectReason::JoinFailure,
            "MIGRATION" => DisconnectReason::Migration,
            "SIGNAL_CLOSE" => DisconnectReason::SignalClose,
            "ROOM_CLOSED" => DisconnectReason::RoomClosed,
            "USER_UNAVAILABLE" => DisconnectReason::UserUnavailable,
            "USER_REJECTED" => DisconnectReason::UserRejected,
            "SIP_TRUNK_FAILURE" => DisconnectReason::SipTrunkFailure,
            _ => DisconnectReason::UnknownReason,
        }
    }

    /// Parse from a webhook `participant.disconnectReason` JSON value, which may
    /// be a number (raw protobuf) or a string (proto3-JSON name). A missing /
    /// null field defaults to `UNKNOWN_REASON` (= 0), matching protobuf's zero
    /// default.
    pub fn parse(v: Option<&serde_json::Value>) -> Self {
        match v {
            Some(serde_json::Value::Number(n)) => {
                DisconnectReason::from_i64(n.as_i64().unwrap_or(0))
            }
            Some(serde_json::Value::String(s)) => DisconnectReason::from_name(s),
            _ => DisconnectReason::UnknownReason,
        }
    }
}

/// Generates credentials for a private voice chat room and inserts the users
/// into the DB as `not_connected`. Faithful port of
/// `getPrivateVoiceChatRoomCredentials`. Returns `{ address: { connection_url } }`.
pub async fn get_private_voice_chat_room_credentials(
    state: &AppState,
    room_id: &str,
    user_addresses: &[String],
) -> Result<BTreeMap<String, serde_json::Value>, crate::http::ApiError> {
    let room_name = private_voice_chat_room_name(room_id);

    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for addr in user_addresses {
        // Upstream generateCredentials: canPublish/canSubscribe true,
        // canUpdateOwnMetadata false, and (cast empty) canPublishSources =
        // [MICROPHONE].
        let mut grants = VideoGrants::join(&room_name);
        grants.can_publish = true;
        grants.can_subscribe = true;
        grants.can_update_own_metadata = false;
        grants.can_publish_sources = Some(vec![TRACK_SOURCE_MICROPHONE.to_string()]);

        let token = AccessToken::new(
            &state.livekit_api_key,
            &state.livekit_api_secret,
            addr,
            grants,
        )
        .to_jwt()
        .map_err(|e| crate::http::ApiError::internal(format!("livekit token: {e}")))?;
        let connection_url = build_adapter_url(&state.livekit_ws_url, &token);
        out.insert(
            addr.clone(),
            serde_json::json!({ "connection_url": connection_url }),
        );
    }

    // Create the room in the database (users => not_connected).
    state
        .voice_db
        .create_voice_chat_room(&room_name, user_addresses)
        .await?;

    Ok(out)
}

/// Ends a private voice chat: removes all users from the room (erroring if the
/// caller was never in it), then deletes the LiveKit room. Returns the
/// addresses that were in the deleted room. Faithful port of
/// `endPrivateVoiceChat`.
pub async fn end_private_voice_chat(
    state: &AppState,
    room_id: &str,
    address: &str,
) -> Result<Vec<String>, crate::http::ApiError> {
    let room_name = private_voice_chat_room_name(room_id);
    let users_in_room = match state
        .voice_db
        .delete_private_voice_chat_user_is_or_was_in(&room_name, address)
        .await
    {
        Ok(users) => users,
        Err(DeleteRoomError::RoomDoesNotExist) => {
            return Err(crate::http::ApiError::not_found(format!(
                "Room {room_id} does not exist"
            )));
        }
        Err(DeleteRoomError::Db(e)) => return Err(e.into()),
    };

    if let Err(e) = state.room_service().delete_room(&room_name).await {
        tracing::warn!(error = %e, room = %room_name, "failed to delete livekit private voice room");
    }
    Ok(users_in_room)
}

/// Checks if a user is currently in a (private) voice chat. Upstream
/// `isUserInVoiceChat`.
pub async fn is_user_in_voice_chat(
    state: &AppState,
    address: &str,
) -> Result<bool, crate::http::ApiError> {
    Ok(state.voice_db.get_room_user_is_in(address).await?.is_some())
}

/// Handles a `participant_joined` webhook for a PRIVATE voice chat room.
/// Faithful port of `handlePrivateParticipantJoined`: if the room is no longer
/// active, tears it down; otherwise marks the user connected and tears down any
/// stale old room they were in.
pub async fn handle_private_participant_joined(
    state: &AppState,
    user_address: &str,
    room_name: &str,
) -> Result<(), crate::http::ApiError> {
    let is_room_active = state.voice_db.is_private_room_active(room_name).await?;
    if !is_room_active {
        tracing::warn!(
            user = user_address,
            room = room_name,
            "user joined an inactive private room, destroying it"
        );
        if let Err(e) = state.room_service().delete_room(room_name).await {
            tracing::warn!(error = %e, room = %room_name, "failed to delete inactive private voice room");
        }
        return Ok(());
    }

    let outcome = state
        .voice_db
        .join_user_to_room(user_address, room_name)
        .await?;

    if outcome.old_room != room_name {
        tracing::debug!(
            user = user_address,
            old_room = %outcome.old_room,
            new_room = room_name,
            "user was in another room when joining, destroying old room"
        );
        if let Err(e) = state.room_service().delete_room(&outcome.old_room).await {
            tracing::warn!(error = %e, room = %outcome.old_room, "failed to delete old private voice room");
        }
    }
    Ok(())
}

/// Handles a `participant_left` webhook for a PRIVATE voice chat room. Faithful
/// port of `handlePrivateParticipantLeft`, branching on the disconnect reason:
/// - `DUPLICATE_IDENTITY` => no-op (the user is re-joining).
/// - `CLIENT_INITIATED` => destroy the room (only two users, voluntary leave)
///   and mark the user `disconnected`.
/// - `ROOM_DELETED` => delete the private voice chat row set (room already
///   gone in LiveKit).
/// - anything else => mark `connection_interrupted` (abrupt disconnect).
pub async fn handle_private_participant_left(
    state: &AppState,
    user_address: &str,
    room_name: &str,
    disconnect_reason: DisconnectReason,
) -> Result<(), crate::http::ApiError> {
    if disconnect_reason == DisconnectReason::DuplicateIdentity {
        return Ok(());
    }

    if disconnect_reason == DisconnectReason::ClientInitiated {
        tracing::debug!(
            user = user_address,
            room = room_name,
            "user left a private room willingly, destroying it"
        );
        if let Err(e) = state.room_service().delete_room(room_name).await {
            tracing::warn!(error = %e, room = %room_name, "failed to delete private voice room on client-initiated leave");
        }
        return state
            .voice_db
            .update_user_status_as_disconnected(user_address, room_name)
            .await
            .map_err(Into::into);
    } else if disconnect_reason == DisconnectReason::RoomDeleted {
        tracing::debug!(
            user = user_address,
            room = room_name,
            "user left a private room because the room was deleted, deleting private voice chat"
        );
        return state
            .voice_db
            .delete_private_voice_chat(room_name)
            .await
            .map_err(Into::into);
    }

    // Treat any other disconnection as an abrupt disconnection.
    state
        .voice_db
        .update_user_status_as_connection_interrupted(user_address, room_name)
        .await
        .map_err(Into::into)
}

/// Deletes expired private voice chats and tears down the rooms that must be
/// destroyed in LiveKit. Faithful port of `expirePrivateVoiceChats`. Intended
/// to be invoked periodically by a background task.
pub async fn expire_private_voice_chats(state: &AppState) -> Result<(), crate::http::ApiError> {
    let expired_room_names = state.voice_db.delete_expired_private_voice_chats().await?;
    for room_name in &expired_room_names {
        if let Err(e) = state.room_service().delete_room(room_name).await {
            tracing::warn!(error = %e, room = %room_name, "failed to delete expired private voice room");
        }
    }
    if !expired_room_names.is_empty() {
        tracing::info!(
            count = expired_room_names.len(),
            "deleted expired private voice chats"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Community voice-chat participant handlers + expiry.
//
// Faithful port of the community half of upstream `voice.ts`:
// `handleCommunityParticipantJoined`, `handleCommunityParticipantLeft`,
// `expireCommunityVoiceChats` and `publishCommunityStreamingEndedEvent`.
// ---------------------------------------------------------------------------

/// Wire-faithful `@dcl/schemas` event type/subtype strings (events/base.ts).
const EVENT_TYPE_STREAMING: &str = "streaming";
const EVENT_SUBTYPE_COMMUNITY_STREAMING_ENDED: &str = "community-streaming-ended";

/// Builds the `CommunityStreamingEnded` event JSON, byte-faithful to the
/// `@dcl/schemas` `CommunityStreamingEndedEvent` shape (a `BaseEvent` with
/// `type`/`subType`/`key`/`timestamp` plus a `{communityId, totalParticipants}`
/// metadata object). Pure so the wire shape is testable without a DB.
fn community_streaming_ended_event(
    community_id: &str,
    participant_count: i64,
    key: &str,
    timestamp_ms: i64,
) -> serde_json::Value {
    serde_json::json!({
        "type": EVENT_TYPE_STREAMING,
        "subType": EVENT_SUBTYPE_COMMUNITY_STREAMING_ENDED,
        "key": key,
        "timestamp": timestamp_ms,
        "metadata": {
            "communityId": community_id,
            "totalParticipants": participant_count,
        }
    })
}

/// Handles a `participant_joined` webhook for a COMMUNITY voice chat room.
/// Faithful port of `handleCommunityParticipantJoined`: simply marks the user
/// `connected`.
pub async fn handle_community_participant_joined(
    state: &AppState,
    user_address: &str,
    room_name: &str,
) -> Result<(), crate::http::ApiError> {
    state
        .voice_db
        .update_community_user_status(user_address, room_name, VoiceChatUserStatus::Connected)
        .await
        .map_err(Into::into)
}

/// Builds and persists a `CommunityStreamingEnded` event into the published-event
/// outbox, byte-faithful to the `@dcl/schemas` `CommunityStreamingEndedEvent`
/// the upstream SNS publisher emits. Faithful port of
/// `publishCommunityStreamingEndedEvent` (incl. the early-return when the room
/// was already deleted, i.e. participant count is 0).
async fn publish_community_streaming_ended_event(
    state: &AppState,
    room_name: &str,
    participant_count: i64,
) {
    if participant_count == 0 {
        tracing::debug!(
            room = room_name,
            "skipping CommunityStreamingEnded since voice chat was already deleted"
        );
        return;
    }

    let community_id = community_id_from_room_name(room_name);
    let now_ms = now_ms();
    let key = format!("community-streaming-ended-{community_id}-{now_ms}");
    let event = community_streaming_ended_event(&community_id, participant_count, &key, now_ms);

    let res = sqlx::query(
        "INSERT INTO published_events (event_key, event_type, event_subtype, payload) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (event_key) DO NOTHING",
    )
    .bind(&key)
    .bind(EVENT_TYPE_STREAMING)
    .bind(EVENT_SUBTYPE_COMMUNITY_STREAMING_ENDED)
    .bind(&event)
    .execute(&state.pool)
    .await;

    match res {
        Ok(_) => tracing::info!(
            community = %community_id,
            participants = participant_count,
            "published CommunityStreamingEnded event"
        ),
        Err(e) => {
            tracing::error!(error = %e, room = room_name, "failed to publish CommunityStreamingEnded event")
        }
    }
}

/// Force-ends a community voice chat regardless of participants. Faithful port
/// of `endCommunityVoiceChat`: snapshot the participant count, delete the
/// LiveKit room, delete the DB room set, then publish `CommunityStreamingEnded`.
pub async fn end_community_voice_chat(
    state: &AppState,
    community_id: &str,
) -> Result<(), crate::http::ApiError> {
    let room_name = crate::livekit::community_voice_chat_room_name(community_id);

    let participant_count = state
        .voice_db
        .get_community_voice_chat_participant_count(&room_name)
        .await?;

    if let Err(e) = state.room_service().delete_room(&room_name).await {
        tracing::warn!(error = %e, room = %room_name, "failed to delete livekit community voice room");
    }

    state
        .voice_db
        .delete_community_voice_chat(&room_name)
        .await?;

    publish_community_streaming_ended_event(state, &room_name, participant_count).await;
    Ok(())
}

/// Handles a `participant_left` webhook for a COMMUNITY voice chat room.
/// Faithful port of `handleCommunityParticipantLeft`, branching on the
/// disconnect reason:
/// - `DUPLICATE_IDENTITY` => no-op (the user is re-joining).
/// - `ROOM_DELETED` => snapshot the participant count, delete the DB room set,
///   publish `CommunityStreamingEnded`.
/// - `CLIENT_INITIATED` => mark `disconnected`; if the leaver was a moderator
///   and no other active moderators remain, destroy the LiveKit room + DB set
///   and publish `CommunityStreamingEnded`.
/// - anything else => mark `connection_interrupted`.
pub async fn handle_community_participant_left(
    state: &AppState,
    user_address: &str,
    room_name: &str,
    disconnect_reason: DisconnectReason,
) -> Result<(), crate::http::ApiError> {
    if disconnect_reason == DisconnectReason::DuplicateIdentity {
        tracing::debug!(
            user = user_address,
            room = room_name,
            "ignoring community disconnect due to duplicate identity"
        );
        return Ok(());
    }

    if disconnect_reason == DisconnectReason::RoomDeleted {
        // Room already deleted in LiveKit: snapshot the count before cleaning DB.
        let participant_count = state
            .voice_db
            .get_community_voice_chat_participant_count(room_name)
            .await?;
        state
            .voice_db
            .delete_community_voice_chat(room_name)
            .await?;
        publish_community_streaming_ended_event(state, room_name, participant_count).await;
        return Ok(());
    }

    if disconnect_reason == DisconnectReason::ClientInitiated {
        state
            .voice_db
            .update_community_user_status(
                user_address,
                room_name,
                VoiceChatUserStatus::Disconnected,
            )
            .await?;

        // Only check for room destruction when a moderator leaves voluntarily.
        let users_in_room = state
            .voice_db
            .get_community_users_in_room(room_name)
            .await?;
        let leaving_is_moderator = users_in_room
            .iter()
            .find(|u| u.address == user_address)
            .map(|u| u.is_moderator)
            .unwrap_or(false);

        if leaving_is_moderator {
            let now = now_ms();
            let remaining_active_moderators = users_in_room
                .iter()
                .filter(|u| {
                    u.is_moderator
                        && u.address != user_address
                        && state.voice_db.is_active_community_user(u, now)
                })
                .count();

            if remaining_active_moderators == 0 {
                tracing::debug!(
                    room = room_name,
                    "no active moderators left in community room, destroying it"
                );
                let participant_count = state
                    .voice_db
                    .get_community_voice_chat_participant_count(room_name)
                    .await?;
                if let Err(e) = state.room_service().delete_room(room_name).await {
                    tracing::warn!(error = %e, room = room_name, "failed to delete community voice room on last-moderator leave");
                }
                state
                    .voice_db
                    .delete_community_voice_chat(room_name)
                    .await?;
                publish_community_streaming_ended_event(state, room_name, participant_count).await;
            }
        }
        return Ok(());
    }

    // Treat any other disconnection as an abrupt disconnection.
    state
        .voice_db
        .update_community_user_status(
            user_address,
            room_name,
            VoiceChatUserStatus::ConnectionInterrupted,
        )
        .await
        .map_err(Into::into)
}

/// Deletes expired community voice chats and tears down their LiveKit rooms,
/// publishing `CommunityStreamingEnded` for each with the pre-deletion
/// participant count. Faithful port of `expireCommunityVoiceChats`.
pub async fn expire_community_voice_chats(state: &AppState) -> Result<(), crate::http::ApiError> {
    // Snapshot participant counts BEFORE deletion (upstream does a bulk count
    // over the currently-active rooms first).
    let active = state
        .voice_db
        .get_all_active_community_voice_chats()
        .await?;
    let community_ids: Vec<String> = active.into_iter().map(|c| c.community_id).collect();
    let room_counts = state
        .voice_db
        .get_bulk_community_voice_chat_participant_count(&community_ids)
        .await?;

    let expired_room_names = state
        .voice_db
        .delete_expired_community_voice_chats()
        .await?;
    for room_name in &expired_room_names {
        let community_id = community_id_from_room_name(room_name);
        tracing::info!(room = %room_name, community = %community_id, "expiring community voice chat room");
        if let Err(e) = state.room_service().delete_room(room_name).await {
            tracing::warn!(error = %e, room = %room_name, "failed to delete expired community voice room");
        }
        let participant_count = room_counts.get(&community_id).copied().unwrap_or(0);
        publish_community_streaming_ended_event(state, room_name, participant_count).await;
    }
    Ok(())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Spawns the voice-chat expiration job. Mirrors upstream's
/// `voiceChatExpirationJob` (every-minute cron, wait-for-completion). Each tick
/// sweeps TTL-expired PRIVATE and COMMUNITY rooms and tears down the LiveKit
/// rooms.
pub fn spawn_expiration_job(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) = expire_private_voice_chats(&state).await {
                tracing::warn!(error = %e, "private voice chat expiration job failed");
            }
            if let Err(e) = expire_community_voice_chats(&state).await {
                tracing::warn!(error = %e, "community voice chat expiration job failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_reason_parses_numeric() {
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!(1))),
            DisconnectReason::ClientInitiated
        );
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!(2))),
            DisconnectReason::DuplicateIdentity
        );
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!(5))),
            DisconnectReason::RoomDeleted
        );
        // Unmodeled numeric reason falls into Other (abrupt-disconnect branch).
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!(99))),
            DisconnectReason::Other(99)
        );
    }

    #[test]
    fn disconnect_reason_parses_proto3_json_name() {
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!("CLIENT_INITIATED"))),
            DisconnectReason::ClientInitiated
        );
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!("DUPLICATE_IDENTITY"))),
            DisconnectReason::DuplicateIdentity
        );
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::json!("ROOM_DELETED"))),
            DisconnectReason::RoomDeleted
        );
    }

    #[test]
    fn missing_disconnect_reason_defaults_to_unknown() {
        assert_eq!(
            DisconnectReason::parse(None),
            DisconnectReason::UnknownReason
        );
        assert_eq!(
            DisconnectReason::parse(Some(&serde_json::Value::Null)),
            DisconnectReason::UnknownReason
        );
    }

    #[test]
    fn community_streaming_ended_event_matches_dcl_schemas_shape() {
        let event = community_streaming_ended_event(
            "my-community",
            7,
            "community-streaming-ended-my-community-1718900000000",
            1_718_900_000_000,
        );
        assert_eq!(event["type"], "streaming");
        assert_eq!(event["subType"], "community-streaming-ended");
        assert_eq!(
            event["key"],
            "community-streaming-ended-my-community-1718900000000"
        );
        assert_eq!(event["timestamp"], 1_718_900_000_000i64);
        assert_eq!(event["metadata"]["communityId"], "my-community");
        assert_eq!(event["metadata"]["totalParticipants"], 7);
        // No extra metadata keys (schema is additionalProperties:false).
        let meta = event["metadata"].as_object().unwrap();
        assert_eq!(meta.len(), 2);
    }
}
