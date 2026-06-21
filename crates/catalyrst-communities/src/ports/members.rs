use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityMember {
    #[serde(rename = "communityId")]
    pub community_id: Uuid,
    #[serde(rename = "memberAddress")]
    pub member_address: String,
    pub role: String,
    #[serde(rename = "joinedAt")]
    pub joined_at: NaiveDateTime,
}

pub struct MembersComponent {
    pool: PgPool,
}

impl MembersComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        community_id: Uuid,
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityMember>, i64), ApiError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, NaiveDateTime)>(
            "SELECT community_id, member_address, role, joined_at \
             FROM community_members WHERE community_id = $1 \
             ORDER BY CASE role WHEN 'owner' THEN 1 WHEN 'moderator' THEN 2 WHEN 'member' THEN 3 ELSE 4 END ASC, \
                      joined_at ASC \
             LIMIT $2 OFFSET $3",
        )
        .bind(community_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM community_members WHERE community_id = $1")
                .bind(community_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let members = rows
            .into_iter()
            .map(
                |(community_id, member_address, role, joined_at)| CommunityMember {
                    community_id,
                    member_address,
                    role: normalize_role(&role),
                    joined_at,
                },
            )
            .collect();
        Ok((members, total))
    }
}

fn normalize_role(role: &str) -> String {
    match role {
        "owner" => "owner",
        "admin" | "mod" | "moderator" => "moderator",
        _ => "member",
    }
    .to_string()
}
