use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityRequest {
    pub id: Uuid,
    #[serde(rename = "communityId")]
    pub community_id: Uuid,
    #[serde(rename = "memberAddress")]
    pub member_address: String,
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: NaiveDateTime,
}

pub struct RequestsComponent {
    pool: PgPool,
}

impl RequestsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_by_community(
        &self,
        community_id: Uuid,
        kind: Option<&str>,
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityRequest>, i64), ApiError> {
        let mut where_sql = "community_id = $1".to_string();
        if kind.is_some() {
            where_sql.push_str(" AND type = $2");
        }
        let limit_idx = if kind.is_some() { 3 } else { 2 };
        let offset_idx = limit_idx + 1;

        let select_sql = format!(
            "SELECT id, community_id, member_address, status, type, created_at, updated_at \
             FROM community_requests WHERE {where_sql} ORDER BY created_at DESC LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );
        let count_sql = format!("SELECT COUNT(*) FROM community_requests WHERE {where_sql}");

        let mut q = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                String,
                NaiveDateTime,
                NaiveDateTime,
            ),
        >(&select_sql)
        .bind(community_id);
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql).bind(community_id);
        if let Some(k) = kind {
            q = q.bind(k);
            cq = cq.bind(k);
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let out = rows.into_iter().map(to_request).collect();
        Ok((out, total))
    }

    pub async fn list_aggregated_by_member(
        &self,
        member_address: &str,
        kind: Option<&str>,
        pagination: &Pagination,
    ) -> Result<(Vec<serde_json::Value>, i64), ApiError> {
        let lower = member_address.to_lowercase();
        let mut where_sql = "r.member_address = $1".to_string();
        if kind.is_some() {
            where_sql.push_str(" AND r.type = $2");
        }
        let limit_idx = if kind.is_some() { 3 } else { 2 };
        let offset_idx = limit_idx + 1;

        let select_sql = format!(
            "SELECT r.id, r.community_id, r.member_address, r.status, r.type, \
                    c.name, c.description, c.owner_address, c.private, c.active, \
                    (SELECT COUNT(*) FROM community_members m WHERE m.community_id = c.id) AS members_count, \
                    COALESCE((SELECT crm.has_thumbnail FROM community_ranking_metrics crm WHERE crm.community_id = c.id), FALSE) AS has_thumbnail \
             FROM community_requests r JOIN communities c ON c.id = r.community_id \
             WHERE {where_sql} ORDER BY r.created_at DESC LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );
        let count_sql = format!(
            "SELECT COUNT(*) FROM community_requests r JOIN communities c ON c.id = r.community_id WHERE {where_sql}"
        );

        let mut q = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                String,
                String,
                String,
                String,
                bool,
                bool,
                i64,
                bool,
            ),
        >(&select_sql)
        .bind(&lower);
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql).bind(&lower);
        if let Some(k) = kind {
            q = q.bind(k);
            cq = cq.bind(k);
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let out = rows
            .into_iter()
            .map(
                |(
                    id,
                    community_id,
                    member_address,
                    status,
                    kind,
                    name,
                    description,
                    owner_address,
                    private,
                    active,
                    members_count,
                    has_thumbnail,
                )| {
                    let privacy = if private { "private" } else { "public" };
                    serde_json::json!({
                        "id": id,
                        "communityId": community_id,
                        "memberAddress": member_address,
                        "type": kind,
                        "status": status,
                        "name": name,
                        "description": description,
                        "ownerAddress": owner_address,
                        "privacy": privacy,
                        "active": active,
                        "membersCount": members_count,
                        "role": "none",
                        "friends": serde_json::Value::Array(vec![]),
                        "_hasThumbnail": has_thumbnail,
                    })
                },
            )
            .collect();
        Ok((out, total))
    }
}

fn to_request(
    (id, community_id, member_address, status, kind, created_at, updated_at): (
        Uuid,
        Uuid,
        String,
        String,
        String,
        NaiveDateTime,
        NaiveDateTime,
    ),
) -> CommunityRequest {
    CommunityRequest {
        id,
        community_id,
        member_address,
        status,
        kind,
        created_at,
        updated_at,
    }
}
