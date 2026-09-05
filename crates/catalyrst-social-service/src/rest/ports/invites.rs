use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::rest::http::ApiError;

#[derive(Debug, Serialize)]
pub struct CommunityInvite {
    pub id: Uuid,
    pub name: String,
}

pub struct InvitesComponent {
    pool: PgPool,
}

impl InvitesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        inviter: &str,
        invitee: &str,
    ) -> Result<Vec<CommunityInvite>, ApiError> {
        let inviter = inviter.to_lowercase();
        let invitee = invitee.to_lowercase();

        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT c.id, c.name FROM communities c \
             JOIN community_members m ON m.community_id = c.id AND m.member_address = $1 \
             WHERE c.active = TRUE \
               AND NOT EXISTS (SELECT 1 FROM community_members m2 WHERE m2.community_id = c.id AND m2.member_address = $2) \
             ORDER BY c.name ASC",
        )
        .bind(&inviter)
        .bind(&invitee)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name)| CommunityInvite { id, name })
            .collect())
    }
}
