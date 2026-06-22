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

#[cfg(test)]
mod tests {
    use super::{normalize_role, CommunityMember};
    use chrono::NaiveDate;
    use uuid::Uuid;

    #[test]
    fn member_serializes_with_unity_wire_keys() {
        let m = CommunityMember {
            community_id: Uuid::nil(),
            member_address: "0xabc".to_string(),
            role: "member".to_string(),
            joined_at: NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        };
        let v = serde_json::to_value(m).unwrap();
        let obj = v.as_object().unwrap();
        for key in ["communityId", "memberAddress", "role", "joinedAt"] {
            assert!(obj.contains_key(key), "member missing {key}");
        }
    }

    #[test]
    fn role_normalizes_to_unity_enum_names() {
        // Unity CommunityMemberRole = { member, moderator, owner, none, unknown }.
        assert_eq!(normalize_role("owner"), "owner");
        assert_eq!(normalize_role("admin"), "moderator");
        assert_eq!(normalize_role("mod"), "moderator");
        assert_eq!(normalize_role("moderator"), "moderator");
        assert_eq!(normalize_role("member"), "member");
        assert_eq!(normalize_role("whatever"), "member");
    }
}
