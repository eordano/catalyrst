use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::http::ApiError;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct ActiveCommunityVoiceChat {
    #[serde(rename = "communityId")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub community_id: Uuid,
    #[serde(rename = "communityName")]
    pub community_name: String,
    #[serde(rename = "communityImage")]
    pub community_image: Option<String>,
    #[serde(rename = "isMember")]
    pub is_member: bool,
    pub positions: Vec<String>,
    pub worlds: Vec<String>,
    #[serde(rename = "participantCount")]
    pub participant_count: i32,
    #[serde(rename = "moderatorCount")]
    pub moderator_count: i32,
}

pub struct ActiveVoiceRow {
    pub community_id: Uuid,
    pub community_name: String,
    pub has_thumbnail: bool,
    pub is_member: bool,
    pub participant_count: i32,
    pub moderator_count: i32,
}

pub struct VoiceComponent {
    pool: PgPool,
}

impl VoiceComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // A non-member only sees a room in a community anyone could have found: public AND listed.
    // Privacy alone let an unlisted community's name, image and live head-count reach any
    // authenticated caller (upstream #476).
    pub async fn active_for_user(
        &self,
        user_address: &str,
    ) -> Result<Vec<ActiveVoiceRow>, ApiError> {
        let lower = user_address.to_lowercase();
        let rows = sqlx::query_as::<_, (Uuid, String, bool, bool, i32, i32)>(
            "SELECT v.community_id, c.name, \
                    COALESCE(crm.has_thumbnail, FALSE) AS has_thumbnail, \
                    EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = v.community_id AND m.member_address = $1) AS is_member, \
                    v.participants, v.moderators \
             FROM community_voice_chats v \
             JOIN communities c ON c.id = v.community_id \
             LEFT JOIN community_ranking_metrics crm ON crm.community_id = v.community_id \
             WHERE c.active = TRUE \
               AND ((c.private = FALSE AND c.unlisted = FALSE) \
                    OR EXISTS (SELECT 1 FROM community_members m WHERE m.community_id = v.community_id AND m.member_address = $1)) \
             ORDER BY v.started_at DESC",
        )
        .bind(&lower)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    community_id,
                    community_name,
                    has_thumbnail,
                    is_member,
                    participant_count,
                    moderator_count,
                )| {
                    ActiveVoiceRow {
                        community_id,
                        community_name,
                        has_thumbnail,
                        is_member,
                        participant_count,
                        moderator_count,
                    }
                },
            )
            .collect())
    }
}
