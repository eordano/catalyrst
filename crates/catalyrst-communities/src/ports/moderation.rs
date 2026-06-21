use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityForModeration {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    pub active: bool,
    pub unlisted: bool,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "membersCount")]
    pub members_count: i64,
}

pub struct ModerationComponent {
    pool: PgPool,
}

impl ModerationComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn all(
        &self,
        search: Option<&str>,
        pagination: &Pagination,
    ) -> Result<(Vec<CommunityForModeration>, i64), ApiError> {
        let mut where_sql = "TRUE".to_string();
        let mut params: Vec<String> = Vec::new();
        if let Some(s) = search {
            params.push(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
            where_sql.push_str(&format!(" AND name ILIKE ${}", params.len()));
        }
        let limit_idx = params.len() + 1;
        let offset_idx = params.len() + 2;

        let select_sql = format!(
            "SELECT id, name, owner_address, active, unlisted, created_at, \
                    (SELECT COUNT(*) FROM community_members m WHERE m.community_id = communities.id) AS members_count \
             FROM communities WHERE {where_sql} ORDER BY created_at DESC LIMIT ${limit_idx} OFFSET ${offset_idx}"
        );
        let count_sql = format!("SELECT COUNT(*) FROM communities WHERE {where_sql}");

        let mut q = sqlx::query_as::<_, (Uuid, String, String, bool, bool, NaiveDateTime, i64)>(
            sqlx::AssertSqlSafe(select_sql),
        );
        let mut cq = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
        for p in &params {
            q = q.bind(p);
            cq = cq.bind(p);
        }
        q = q.bind(pagination.limit).bind(pagination.offset);
        let rows = q.fetch_all(&self.pool).await?;
        let total = cq.fetch_one(&self.pool).await.unwrap_or(0);

        let out = rows
            .into_iter()
            .map(
                |(id, name, owner_address, active, unlisted, created_at, members_count)| {
                    CommunityForModeration {
                        id,
                        name,
                        owner_address,
                        active,
                        unlisted,
                        created_at,
                        members_count,
                    }
                },
            )
            .collect();
        Ok((out, total))
    }
}
