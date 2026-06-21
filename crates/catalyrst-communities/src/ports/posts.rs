use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{ApiError, Pagination};

#[derive(Debug, Serialize)]
pub struct CommunityPost {
    pub id: Uuid,
    #[serde(rename = "communityId")]
    pub community_id: Uuid,
    #[serde(rename = "authorAddress")]
    pub author_address: String,
    pub content: String,
    #[serde(rename = "createdAt")]
    pub created_at: NaiveDateTime,
    #[serde(rename = "likesCount")]
    pub likes_count: i64,
    #[serde(rename = "isLikedByUser")]
    pub liked_by_me: bool,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

pub struct PostsComponent {
    pool: PgPool,
}

impl PostsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        community_id: Uuid,
        pagination: &Pagination,
        as_user: Option<&str>,
    ) -> Result<(Vec<CommunityPost>, i64), ApiError> {
        let user = as_user.map(|s| s.to_lowercase()).unwrap_or_default();

        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, NaiveDateTime, i64, bool)>(
            "SELECT p.id, p.community_id, p.author_address, p.content, p.created_at, \
                    COALESCE((SELECT COUNT(*) FROM community_post_likes l WHERE l.post_id = p.id), 0)::int8 AS likes_count, \
                    EXISTS (SELECT 1 FROM community_post_likes l WHERE l.post_id = p.id AND l.user_address = $4) AS liked_by_me \
             FROM community_posts p \
             WHERE p.community_id = $1 \
             ORDER BY p.created_at DESC \
             LIMIT $2 OFFSET $3",
        )
        .bind(community_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .bind(&user)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM community_posts WHERE community_id = $1")
                .bind(community_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        let posts = rows
            .into_iter()
            .map(
                |(
                    id,
                    community_id,
                    author_address,
                    content,
                    created_at,
                    likes_count,
                    liked_by_me,
                )| {
                    CommunityPost {
                        id,
                        community_id,
                        author_address,
                        content,
                        created_at,
                        likes_count,
                        liked_by_me,
                        kind: "POST",
                    }
                },
            )
            .collect();
        Ok((posts, total))
    }
}
