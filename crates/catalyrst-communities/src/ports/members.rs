use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};
use crate::ports::profiles::NameColor;

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CommunityMember {
    #[serde(rename = "communityId")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub community_id: Uuid,
    #[serde(rename = "memberAddress")]
    pub member_address: String,
    pub role: String,
    #[serde(rename = "joinedAt")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub joined_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CommunityMemberWire {
    #[serde(flatten)]
    pub base: CommunityMember,
    pub name: String,
    #[serde(rename = "profilePictureUrl")]
    pub profile_picture_url: String,
    #[serde(rename = "hasClaimedName")]
    pub has_claimed_name: bool,
    #[serde(rename = "nameColor")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name_color: Option<NameColor>,
    #[serde(rename = "friendshipStatus")]
    pub friendship_status: i32,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct CommunityMemberV2Wire {
    #[serde(flatten)]
    pub base: CommunityMember,
    #[serde(rename = "friendshipStatus")]
    pub friendship_status: i32,
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

        let members = rows.into_iter().map(row_to_member).collect();
        Ok((members, total))
    }

    pub async fn list_online(
        &self,
        community_id: Uuid,
        online: &[String],
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityMember>, i64), ApiError> {
        let filter: Vec<String> = online.iter().map(|a| a.to_lowercase()).collect();

        let rows = sqlx::query_as::<_, (Uuid, String, String, NaiveDateTime)>(
            "SELECT community_id, member_address, role, joined_at \
             FROM community_members WHERE community_id = $1 \
               AND member_address = ANY($2::text[]) \
             ORDER BY CASE role WHEN 'owner' THEN 1 WHEN 'moderator' THEN 2 WHEN 'member' THEN 3 ELSE 4 END ASC, \
                      joined_at ASC \
             LIMIT $3 OFFSET $4",
        )
        .bind(community_id)
        .bind(&filter)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM community_members WHERE community_id = $1 AND member_address = ANY($2::text[])",
        )
        .bind(community_id)
        .bind(&filter)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let members = rows.into_iter().map(row_to_member).collect();
        Ok((members, total))
    }
}

fn row_to_member(
    (community_id, member_address, role, joined_at): (Uuid, String, String, NaiveDateTime),
) -> CommunityMember {
    CommunityMember {
        community_id,
        member_address,
        role: normalize_role(&role),
        joined_at,
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
        assert_eq!(normalize_role("owner"), "owner");
        assert_eq!(normalize_role("admin"), "moderator");
        assert_eq!(normalize_role("mod"), "moderator");
        assert_eq!(normalize_role("moderator"), "moderator");
        assert_eq!(normalize_role("member"), "member");
        assert_eq!(normalize_role("whatever"), "member");
    }
}
