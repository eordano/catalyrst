#[derive(Debug, serde::Serialize)]
pub struct SceneAdapterResponse {
    pub adapter: String,
}

#[derive(Debug, serde::Serialize)]
pub struct SceneParticipantsResponse {
    pub ok: bool,
    pub data: SceneParticipantsData,
}

#[derive(Debug, serde::Serialize)]
pub struct SceneParticipantsData {
    pub addresses: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SceneStreamAccessResponse {
    pub streaming_url: String,
    pub streaming_key: String,
    pub created_at: i64,
    pub ends_at: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct VoiceChatStatusResponse {
    pub is_user_in_voice_chat: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct CommunityVoiceChatStatusResponse {
    pub active: bool,
    pub participant_count: i64,
    pub moderator_count: i64,
}
