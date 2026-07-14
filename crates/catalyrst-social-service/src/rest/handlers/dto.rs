use chrono::NaiveDateTime;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::rest::ports::voice::ActiveCommunityVoiceChat;

#[derive(Debug, Serialize, ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct VoiceChatStatus {
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "participantCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub participant_count: i64,
    #[serde(rename = "moderatorCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub moderator_count: i64,
}

impl VoiceChatStatus {
    pub fn idle() -> Self {
        Self {
            is_active: false,
            participant_count: 0,
            moderator_count: 0,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CommunityListItem {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub privacy: String,
    pub active: bool,
    pub unlisted: bool,
    #[serde(rename = "membersCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub members_count: i64,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: NaiveDateTime,
    #[serde(rename = "isLive")]
    pub is_live: bool,
    #[serde(rename = "voiceChatStatus")]
    pub voice_chat_status: VoiceChatStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub role: Option<String>,
    #[serde(rename = "isBanned")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub is_banned: Option<bool>,
    #[serde(rename = "thumbnailUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub thumbnail_url: Option<String>,
    #[serde(rename = "ownerName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub owner_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>)]
    #[cfg_attr(feature = "ts", ts(optional, type = "Array<unknown>"))]
    pub friends: Option<Vec<serde_json::Value>>,
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub has_thumbnail: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CommunityDetail {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub privacy: String,
    pub active: bool,
    pub unlisted: bool,
    #[serde(rename = "membersCount")]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub members_count: i64,
    #[serde(rename = "createdAt")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: NaiveDateTime,
    #[serde(rename = "isLive")]
    pub is_live: bool,
    #[serde(rename = "voiceChatStatus")]
    pub voice_chat_status: VoiceChatStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub role: Option<String>,
    #[serde(rename = "isBanned")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub is_banned: Option<bool>,
    #[serde(rename = "thumbnailUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub thumbnail_url: Option<String>,
    #[serde(rename = "ownerName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub owner_name: Option<String>,
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub has_thumbnail: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct ActiveVoiceChatsData {
    #[serde(rename = "activeChats")]
    pub active_chats: Vec<ActiveCommunityVoiceChat>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: usize,
}
