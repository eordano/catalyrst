use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use catalyrst_authenticated_principal::{
    ClaimedCommunityRoleNameNobodyHasVerified, ClaimedWalletAddressNobodyHasVerified,
};
use catalyrst_types::is_eth_address;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::extract::{device_identifier, get_request_ip};
use crate::handlers::responses::{CommunityVoiceChatStatusResponse, VoiceChatStatusResponse};
use crate::http::{service_unavailable, unauthorized, ApiError};
use crate::livekit::{build_adapter_url, community_voice_chat_room_name, join_grants, AccessToken};
use crate::ports::player_connection::UpsertPlayerConnection;
use crate::util::now_ms;
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
    .await
    .map_err(|e| ApiError::http(e.status, e.message))?;
    let identity = sf.signer.as_str().to_string();

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

    let mut grants = join_grants(&state.private_messages_room_id);
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
) -> Result<Json<VoiceChatStatusResponse>, ApiError> {
    let address = address.to_lowercase();

    let is_user_in_voice_chat = crate::voice_logic::is_user_in_voice_chat(&state, &address).await?;

    Ok(Json(VoiceChatStatusResponse {
        is_user_in_voice_chat,
    }))
}

/// Request body for community voice chat create/join.
///
/// # Trust boundary
///
/// `user_address` and `user_role` are taken **verbatim from the request body**.
/// Nothing in this crate verifies that the caller controls `user_address`, that
/// `user_role` reflects any stored community membership, or that the two are
/// even related. `user_role` alone decides speaker rights and the moderator flag
/// persisted by [`community_voice_chat_create_or_join`].
///
/// The only thing gating that is the shared `COMMS_GATEKEEPER_AUTH_TOKEN` bearer
/// checked by `crate::voice_auth_layer`. See the boundary note on
/// [`community_voice_chat_create_or_join`] for the blast radius.
///
/// # How that boundary is stated in the type system
///
/// The two untrusted fields carry [`ClaimedWalletAddressNobodyHasVerified`] and
/// [`ClaimedCommunityRoleNameNobodyHasVerified`] rather than `String`. Neither type
/// has a conversion into
/// `catalyrst_authenticated_principal::VerifiedWalletAddress`, nor
/// into any community membership standing, nor `AsRef<str>`, nor a `PartialEq`
/// against a verified value. Code that treats one of these claims as proven
/// therefore fails to compile instead of failing silently, and the one exit,
/// `as_unverified_text()`, is spelled out at every use site.
///
/// This is documentation, not enforcement: the values are still trusted verbatim,
/// exactly as before. Both types are `#[serde(transparent)]` newtypes over `String`,
/// so the accepted JSON is byte-for-byte unchanged.
#[derive(Debug, Deserialize)]
pub struct CommunityVoiceChatBody {
    pub community_id: String,
    pub user_address: ClaimedWalletAddressNobodyHasVerified,
    pub user_role: Option<ClaimedCommunityRoleNameNobodyHasVerified>,
    pub action: Option<String>,
    pub profile_data: Option<serde_json::Value>,
}

/// The role name assumed when the gatekeeper service sends no `user_role` at all.
///
/// Still a claim: nobody checked it either, it is simply the claim the absence of
/// the field is read as. It is not in the tier vocabulary of any database table.
const CLAIMED_ROLE_NAME_ASSUMED_WHEN_THE_BODY_OMITS_ONE: &str = "none";

/// The pair of assertions a gatekeeper service makes about **somebody else** when it
/// creates or joins a community voice room: which wallet it is acting for, and what
/// community role it says that wallet holds.
///
/// # What a value of this type proves
///
/// That two strings arrived in the body of a request bearing the shared
/// `COMMS_GATEKEEPER_AUTH_TOKEN`, and that the address is `0x` followed by 40 hex
/// digits once lowercased. Nothing else. It does **not** prove that the caller
/// controls that address, that the role was ever stored against it, that the two are
/// related, or that the community exists.
///
/// # Why this is a type and not two local `String`s
///
/// Before this existed the handler lowercased both fields into bare `String`s, and
/// from that line onward the claimed wallet was indistinguishable from a wallet
/// proven by an ADR-44 signature -- which is precisely how the same word ends up
/// meaning two different things on two paths. Carrying the claims in named types all
/// the way to the LiveKit call makes each escape of the untrusted text visible.
///
/// # What it deliberately does not do
///
/// It performs no database lookup and no authorization. See the boundary note on
/// [`community_voice_chat_create_or_join`]: comms is a downstream executor here, and
/// that is pinned by `crates/catalyrst-comms/tests/voice_auth_fail_closed.rs`.
#[derive(Debug)]
struct ServiceClaimedCommunityRole {
    claimed_wallet_address_lowercased: ClaimedWalletAddressNobodyHasVerified,
    claimed_community_role_name_lowercased: ClaimedCommunityRoleNameNobodyHasVerified,
}

impl ServiceClaimedCommunityRole {
    /// Lowercase both claims and shape-check the address, exactly as the handler did
    /// inline before. The caller must have already rejected an empty `community_id`,
    /// because that check reports first and its message is part of the API.
    fn shape_check_only_from_the_request_body(
        body: &CommunityVoiceChatBody,
    ) -> Result<Self, ApiError> {
        let claimed_wallet_address_lowercased =
            ClaimedWalletAddressNobodyHasVerified::from_untrusted_text(
                body.user_address.as_unverified_text().to_lowercase(),
            );
        if !is_eth_address(claimed_wallet_address_lowercased.as_unverified_text()) {
            return Err(ApiError::bad_request(
                "The property user_address is invalid",
            ));
        }

        let claimed_community_role_name_lowercased =
            ClaimedCommunityRoleNameNobodyHasVerified::from_untrusted_text(
                body.user_role
                    .as_ref()
                    .map(ClaimedCommunityRoleNameNobodyHasVerified::as_unverified_text)
                    .unwrap_or(CLAIMED_ROLE_NAME_ASSUMED_WHEN_THE_BODY_OMITS_ONE)
                    .to_lowercase(),
            );

        Ok(Self {
            claimed_wallet_address_lowercased,
            claimed_community_role_name_lowercased,
        })
    }

    /// The lowercased address the gatekeeper service named. Used as the LiveKit
    /// participant identity and as the `community_voice_chat_users` key.
    fn claimed_wallet_address_as_unverified_text(&self) -> &str {
        self.claimed_wallet_address_lowercased.as_unverified_text()
    }

    /// See the free function of the same name.
    fn the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(&self) -> bool {
        the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(
            &self.claimed_community_role_name_lowercased,
        )
    }
}

/// Whether the role a gatekeeper service **claimed** is one this handler grants
/// LiveKit publish rights and the persisted moderator flag for.
///
/// Formerly `is_moderator_role`. The predicate body is byte-identical -- this remains
/// `matches!(.., "owner" | "moderator")` and is not a behaviour change. What changed
/// is that it now takes [`ClaimedCommunityRoleNameNobodyHasVerified`], so a verified
/// membership standing cannot be passed to it by accident and a claim cannot be
/// passed to a predicate that expects a standing.
///
/// The long name is the point: this is one of **six** unrelated things called `role`
/// in this workspace, and one of at least four moderation predicates that disagree
/// about which strings count. Note what it still does not accept -- `"mod"`, which
/// `catalyrst-social-service`'s `role_has_invite_users` does accept, and `"admin"`,
/// which its `has_moderation_permission` does accept. Those vocabularies disagree
/// today; this change names the disagreement and deliberately does not resolve it.
fn the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(
    claimed_community_role_name: &ClaimedCommunityRoleNameNobodyHasVerified,
) -> bool {
    matches!(
        claimed_community_role_name.as_unverified_text(),
        "owner" | "moderator"
    )
}

/// Build the LiveKit participant metadata blob for a community voice join.
///
/// The `role` it writes is the gatekeeper service's **claim**, echoed to every other
/// participant in the room; hence the parameter type. Nothing here consults a
/// database, so nothing here can produce a verified standing.
fn community_join_metadata(
    claimed_community_role_name: &ClaimedCommunityRoleNameNobodyHasVerified,
    is_speaker: bool,
    profile_data: Option<&serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "role".into(),
        serde_json::json!(claimed_community_role_name.as_unverified_text()),
    );
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

/// Create or join a community voice chat room.
///
/// # Trust boundary -- INTENTIONAL, do not "fix" without reading this
///
/// This handler performs **no** per-user authorization. It does not verify a
/// signature over `user_address`, and it does not look up the caller's real role
/// in `community_id`; it grants publish rights and the moderator flag from the
/// `user_role` string in the request body (see [`CommunityVoiceChatBody`]).
///
/// The sole gate is the shared `COMMS_GATEKEEPER_AUTH_TOKEN` bearer enforced by
/// `crate::voice_auth_layer` for every `*voice-chat*` path. That is a single
/// static secret shared by all platform services that call comms.
///
/// ## Blast radius of a token leak
///
/// Anyone holding that one token can, for **any** community and **any** wallet
/// address, without owning either:
///
/// - self-grant `owner`/`moderator` and join as a speaker with publish rights,
/// - persist the moderator flag against an arbitrary address via
///   [`crate::voice_db::VoiceDb::join_user_to_community_room`],
/// - and, through the sibling handlers gated by [`require_community_and_user`],
///   promote, demote, kick, and mute arbitrary participants.
///
/// There is no per-community blast-radius containment: the token is not scoped
/// to a community, a service, or an address, so one leak is moderator authority
/// over every community at once, and the audit trail attributes each action to
/// the spoofed address rather than to the token holder. Rotating
/// `COMMS_GATEKEEPER_AUTH_TOKEN` is the only remediation.
///
/// ## Why it is this way
///
/// The upstream contract puts community-role resolution in the calling platform
/// service, which has already resolved membership before it reaches comms;
/// comms is deliberately a downstream executor here, not an authorizer.
///
/// The intent is pinned by
/// `crates/catalyrst-comms/tests/voice_auth_fail_closed.rs` --
/// `configured_service_token_still_gates_on_the_bearer` asserts that a request
/// bearing the correct token reaches this handler (400 on a malformed body)
/// while a missing or wrong bearer is rejected 401, and
/// `unconfigured_service_token_refuses_the_bearer_gated_voice_routes` asserts
/// the route answers 503 rather than opening up when the token is unset. If you
/// add real per-user authorization here, those tests are what you must revisit.
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
    let claimed = ServiceClaimedCommunityRole::shape_check_only_from_the_request_body(&body)?;
    let claimed_role_grants_speaker_rights =
        claimed.the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights();
    let action = body.action.as_deref().unwrap_or("join").to_lowercase();
    let is_creating = action == "create";
    let is_speaker = is_creating && claimed_role_grants_speaker_rights;

    let room_name = community_voice_chat_room_name(&body.community_id);

    let metadata = community_join_metadata(
        &claimed.claimed_community_role_name_lowercased,
        is_speaker,
        body.profile_data.as_ref(),
    );

    let mut grants = join_grants(&room_name);
    grants.can_publish = is_speaker;
    grants.can_subscribe = true;
    grants.can_update_own_metadata = false;

    let token = AccessToken::new(
        &state.livekit_api_key,
        &state.livekit_api_secret,
        claimed.claimed_wallet_address_as_unverified_text(),
        grants,
    )
    .with_metadata(serde_json::Value::Object(metadata).to_string())
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("livekit token: {e}")))?;

    let connection_url = build_adapter_url(&state.livekit_ws_url, &token);

    state
        .voice_db
        .join_user_to_community_room(
            claimed.claimed_wallet_address_as_unverified_text(),
            &room_name,
            claimed_role_grants_speaker_rights,
        )
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

pub async fn community_voice_chat_status(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<CommunityVoiceChatStatusResponse>, ApiError> {
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
    Ok(Json(CommunityVoiceChatStatusResponse {
        active,
        participant_count,
        moderator_count,
    }))
}

#[derive(Debug, Deserialize)]
pub struct BulkCommunityStatusBody {
    pub community_ids: Vec<String>,
}

const MAX_BULK_COMMUNITY_IDS: usize = 100;

fn validate_bulk_community_ids(ids: &[String]) -> Result<(), ApiError> {
    if ids.len() > MAX_BULK_COMMUNITY_IDS {
        return Err(ApiError::bad_request(format!(
            "community_ids must contain at most {MAX_BULK_COMMUNITY_IDS} items"
        )));
    }
    Ok(())
}

pub async fn community_voice_chat_bulk_status(
    State(state): State<AppState>,
    Json(body): Json<BulkCommunityStatusBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_bulk_community_ids(&body.community_ids)?;
    let room_names: Vec<String> = body
        .community_ids
        .iter()
        .map(|id| community_voice_chat_room_name(id))
        .collect();
    let by_room = state
        .voice_db
        .get_bulk_community_voice_chat_status(&room_names)
        .await?;
    let data: Vec<_> = body
        .community_ids
        .iter()
        .zip(&room_names)
        .map(|(id, room)| {
            let (participant_count, moderator_count) = by_room.get(room).copied().unwrap_or((0, 0));
            serde_json::json!({
                "community_id": id,
                "active": moderator_count > 0,
                "participant_count": participant_count,
                "moderator_count": moderator_count,
            })
        })
        .collect();
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

/// Shared argument check for the community voice moderation handlers
/// (request/reject-to-speak, promote/demote speaker, kick, mute).
///
/// # Trust boundary -- INTENTIONAL, do not "fix" without reading this
///
/// This is a **shape** check, not an authorization check: it only asserts the
/// two path parameters are non-empty and lowercases the address. No caller
/// identity is established, and the target `user_address` is never checked
/// against the caller. Every handler below that calls this therefore acts on an
/// arbitrary address in an arbitrary community on the say-so of whoever holds
/// the shared `COMMS_GATEKEEPER_AUTH_TOKEN` bearer that
/// `crate::voice_auth_layer` checked.
///
/// A leak of that single token is moderator authority over every community at
/// once. See the full blast-radius note on
/// [`community_voice_chat_create_or_join`]; the intent is pinned by
/// `crates/catalyrst-comms/tests/voice_auth_fail_closed.rs`.
///
/// # Why it returns a claim
///
/// It was named `require_community_and_user` and returned a bare `String`, which
/// read like an authorization step and produced a value indistinguishable from a
/// verified address. The new name says what it does, and
/// [`ClaimedWalletAddressNobodyHasVerified`] says what the returned address is worth:
/// nothing, beyond being non-empty and lowercased. The checks themselves, their
/// order, and their messages are unchanged.
fn reject_empty_community_identifier_and_wallet_address_path_parameters(
    community_id: &str,
    claimed_user_address: &str,
) -> Result<ClaimedWalletAddressNobodyHasVerified, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter communityId is required",
        ));
    }
    let addr = claimed_user_address.to_lowercase();
    if addr.trim().is_empty() {
        return Err(ApiError::bad_request(
            "The parameter userAddress is required",
        ));
    }
    Ok(ClaimedWalletAddressNobodyHasVerified::from_untrusted_text(
        addr,
    ))
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
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(true));
    merge_metadata(
        &state,
        &room_name,
        claimed_target.as_unverified_text(),
        patch,
    )
    .await?;
    Ok(Json(serde_json::json!({
        "message": "Request to speak sent successfully"
    })))
}

pub async fn community_reject_speak_request(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("isRequestingToSpeak".into(), serde_json::json!(false));
    merge_metadata(
        &state,
        &room_name,
        claimed_target.as_unverified_text(),
        patch,
    )
    .await?;
    Ok(Json(serde_json::json!({
        "message": "Speak request rejected successfully"
    })))
}

pub async fn community_promote_speaker(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);

    state
        .room_service()
        .update_participant(
            &room_name,
            claimed_target.as_unverified_text(),
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
    merge_metadata(
        &state,
        &room_name,
        claimed_target.as_unverified_text(),
        patch,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "message": "User promoted to speaker successfully"
    })))
}

pub async fn community_demote_speaker(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);

    state
        .room_service()
        .update_participant(
            &room_name,
            claimed_target.as_unverified_text(),
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
    merge_metadata(
        &state,
        &room_name,
        claimed_target.as_unverified_text(),
        patch,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "message": "User demoted to listener successfully"
    })))
}

pub async fn community_kick_player(
    State(state): State<AppState>,
    Path((community_id, user_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_livekit(&state)?;
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);

    if let Err(e) = state
        .room_service()
        .remove_participant(&room_name, claimed_target.as_unverified_text())
        .await
    {
        tracing::warn!(error = %e, room = %room_name, addr = %claimed_target.as_unverified_text(), "failed to remove community voice participant");
    }

    sqlx::query("DELETE FROM community_voice_chat_users WHERE address = $1 AND room_name = $2")
        .bind(claimed_target.as_unverified_text())
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
    let claimed_target = reject_empty_community_identifier_and_wallet_address_path_parameters(
        &community_id,
        &user_address,
    )?;
    let room_name = community_voice_chat_room_name(&community_id);
    let mut patch = serde_json::Map::new();
    patch.insert("muted".into(), serde_json::json!(body.muted));
    merge_metadata(
        &state,
        &room_name,
        claimed_target.as_unverified_text(),
        patch,
    )
    .await?;
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
        assert_eq!(
            full.user_role
                .as_ref()
                .map(ClaimedCommunityRoleNameNobodyHasVerified::as_unverified_text),
            Some("owner")
        );
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
    fn bulk_community_ids_bounded_at_100() {
        let at_limit: Vec<String> = (0..100).map(|i| format!("c{i}")).collect();
        assert!(validate_bulk_community_ids(&at_limit).is_ok());

        let over_limit: Vec<String> = (0..101).map(|i| format!("c{i}")).collect();
        let err = validate_bulk_community_ids(&over_limit).unwrap_err();
        assert_eq!(err.code, 400);
    }

    fn claimed_role(raw: &str) -> ClaimedCommunityRoleNameNobodyHasVerified {
        ClaimedCommunityRoleNameNobodyHasVerified::from_untrusted_text(raw)
    }

    fn community_voice_chat_body(
        user_address: &str,
        user_role: Option<&str>,
    ) -> CommunityVoiceChatBody {
        CommunityVoiceChatBody {
            community_id: "c1".into(),
            user_address: ClaimedWalletAddressNobodyHasVerified::from_untrusted_text(user_address),
            user_role: user_role.map(claimed_role),
            action: None,
            profile_data: None,
        }
    }

    /// The former `create_action_with_moderator_role_is_speaker`, unchanged in what it
    /// asserts: the accepted set is exactly `{"owner", "moderator"}`.
    #[test]
    fn the_claimed_role_that_grants_speaker_rights_is_still_exactly_owner_or_moderator() {
        assert!(
            the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(&claimed_role(
                "owner"
            ))
        );
        assert!(
            the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(&claimed_role(
                "moderator"
            ))
        );
        assert!(
            !the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(&claimed_role(
                "member"
            ))
        );
        assert!(
            !the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(&claimed_role(
                "none"
            ))
        );
    }

    /// The four moderation vocabularies in this workspace disagree, and this file is one
    /// of them. `"mod"` is accepted by `catalyrst-social-service`'s
    /// `role_has_invite_users` and `"admin"` by its `has_moderation_permission`; neither
    /// grants voice speaker rights here. That divergence is pre-existing and deliberately
    /// left alone -- this test exists so a later unification cannot change it silently.
    #[test]
    fn the_claimed_role_predicate_still_rejects_the_vocabularies_other_crates_accept() {
        for rejected in ["mod", "admin", "Owner", "OWNER", "banned", ""] {
            assert!(
                !the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights(
                    &claimed_role(rejected)
                ),
                "{rejected:?} must not grant voice speaker rights"
            );
        }
    }

    /// The claims are lowercased before the predicate sees them, so a gatekeeper service
    /// sending `"Owner"` does get speaker rights -- via normalization, not via the
    /// predicate. Pinning both halves keeps the two facts from being conflated.
    #[test]
    fn the_shape_check_lowercases_both_claims_exactly_as_the_handler_did_inline() {
        let claimed = ServiceClaimedCommunityRole::shape_check_only_from_the_request_body(
            &community_voice_chat_body("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", Some("OWNER")),
        )
        .expect("a well-formed address and any role name pass the shape check");

        assert_eq!(
            claimed.claimed_wallet_address_as_unverified_text(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert!(claimed.the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights());
    }

    /// An omitted `user_role` is read as the claim `"none"`, which grants nothing.
    #[test]
    fn an_omitted_claimed_role_is_read_as_none_and_grants_no_speaker_rights() {
        let claimed = ServiceClaimedCommunityRole::shape_check_only_from_the_request_body(
            &community_voice_chat_body("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None),
        )
        .expect("user_role is optional");

        assert_eq!(
            claimed
                .claimed_community_role_name_lowercased
                .as_unverified_text(),
            CLAIMED_ROLE_NAME_ASSUMED_WHEN_THE_BODY_OMITS_ONE
        );
        assert!(!claimed.the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights());
    }

    /// The shape check is a shape check: the address must look like an address, and the
    /// role name is accepted whatever it says, because nothing in this crate resolves it.
    #[test]
    fn the_shape_check_rejects_a_malformed_address_and_accepts_any_role_text() {
        let bad_address = ServiceClaimedCommunityRole::shape_check_only_from_the_request_body(
            &community_voice_chat_body("not-an-address", Some("owner")),
        )
        .unwrap_err();
        assert_eq!(bad_address.code, 400);

        let nonsense_role = ServiceClaimedCommunityRole::shape_check_only_from_the_request_body(
            &community_voice_chat_body(
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                Some("owner'; DROP TABLE community_members;--"),
            ),
        )
        .expect("a claim is accepted whatever it says; it decides nothing on its own");
        assert!(
            !nonsense_role.the_gatekeeper_service_claimed_a_role_that_grants_voice_speaker_rights()
        );
    }

    /// The path-parameter shape check keeps its two messages, their order, and the
    /// lowercasing; only its name and return type changed.
    #[test]
    fn the_path_parameter_shape_check_keeps_its_messages_order_and_lowercasing() {
        let empty_community =
            reject_empty_community_identifier_and_wallet_address_path_parameters("  ", "0xAbC")
                .unwrap_err();
        assert_eq!(empty_community.code, 400);

        let empty_address =
            reject_empty_community_identifier_and_wallet_address_path_parameters("c1", "   ")
                .unwrap_err();
        assert_eq!(empty_address.code, 400);

        let ok =
            reject_empty_community_identifier_and_wallet_address_path_parameters("c1", "0xAbC")
                .expect("a non-empty community and address pass");
        assert_eq!(ok.as_unverified_text(), "0xabc");
    }

    #[test]
    fn community_join_metadata_create_owner_is_speaker() {
        let md = community_join_metadata(&claimed_role("owner"), true, None);
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
        let md = community_join_metadata(&claimed_role("member"), false, Some(&profile));
        assert_eq!(md["isSpeaker"], false);
        assert_eq!(md["name"], "Foo");
        assert_eq!(md["hasClaimedName"], true);
        assert_eq!(md["profilePictureUrl"], "http://x/y.png");
    }

    #[test]
    fn community_join_metadata_omits_absent_profile_keys() {
        let profile = serde_json::json!({ "name": "Foo" });
        let md = community_join_metadata(&claimed_role("member"), false, Some(&profile));
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
