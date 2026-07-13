use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

use crate::auth_chain::require_signer;
use crate::handlers::communities::thumbnail_url;
use crate::http::ApiError;
use crate::ports::voice::ActiveCommunityVoiceChat;
use crate::AppState;

pub async fn get_active_voice_chats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = require_signer(&headers, "get", "/v1/community-voice-chats/active")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let rows = state.voice.active_for_user(&signer).await?;

    let active: Vec<ActiveCommunityVoiceChat> = rows
        .into_iter()
        .map(|r| {
            let community_image = if r.has_thumbnail {
                Some(thumbnail_url(&state.cdn_url, &r.community_id.to_string()))
            } else {
                None
            };
            ActiveCommunityVoiceChat {
                community_id: r.community_id,
                community_name: r.community_name,
                community_image,
                is_member: r.is_member,
                positions: Vec::new(),
                worlds: Vec::new(),
                participant_count: r.participant_count,
                moderator_count: r.moderator_count,
            }
        })
        .collect();
    let total = active.len();
    Ok(Json(serde_json::json!({
        "data": { "activeChats": active, "total": total }
    })))
}
