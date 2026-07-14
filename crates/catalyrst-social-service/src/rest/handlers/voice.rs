use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::communities::thumbnail_url;
use crate::rest::handlers::dto::ActiveVoiceChatsData;
use crate::rest::handlers::error::{CommError, SignedFetchGateBody};
use crate::rest::http::EnvelopeData;
use crate::rest::ports::voice::ActiveCommunityVoiceChat;
use crate::rest::AppState;

#[utoipa::path(
    get,
    path = "/v1/community-voice-chats/active",
    tag = "voice",
    responses(
        (status = 200, body = EnvelopeData<ActiveVoiceChatsData>),
        (status = 400, body = SignedFetchGateBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_active_voice_chats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EnvelopeData<ActiveVoiceChatsData>>, CommError> {
    let signer = require_signer(&headers, "get", "/v1/community-voice-chats/active").await?;
    let rows = state.voice.active_for_user(signer.as_str()).await?;

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
    Ok(Json(EnvelopeData {
        data: ActiveVoiceChatsData {
            active_chats: active,
            total,
        },
    }))
}
