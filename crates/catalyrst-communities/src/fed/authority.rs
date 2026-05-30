use sqlx::PgPool;

use crate::http::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Banned,
    None,
    Member,
    Mod,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "mod" | "moderator" => Some(Role::Mod),
            "member" => Some(Role::Member),
            "banned" => Some(Role::Banned),
            "none" | "" => Some(Role::None),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Mod => "mod",
            Role::Member => "member",
            Role::Banned => "banned",
            Role::None => "none",
        }
    }
}

pub async fn load_role(pool: &PgPool, community_id: &str, member: &str) -> Result<Role, ApiError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM community_role_current WHERE community_id = $1 AND member = $2",
    )
    .bind(community_id)
    .bind(member.to_ascii_lowercase())
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.and_then(|(r,)| Role::parse(&r)).unwrap_or(Role::None))
}

pub async fn require_min_role(
    pool: &PgPool,
    community_id: &str,
    signer: &str,
    min: Role,
) -> Result<Role, ApiError> {
    let actual = load_role(pool, community_id, signer).await?;
    if actual == Role::Banned {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            403,
            "Forbidden: banned from this community",
        )));
    }
    if actual < min {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            403,
            format!(
                "Forbidden: signer role {} below required {}",
                actual.as_str(),
                min.as_str()
            ),
        )));
    }
    Ok(actual)
}

pub fn can_grant(actor: Role, target_role: Role) -> bool {
    match actor {
        Role::Owner => matches!(
            target_role,
            Role::Admin | Role::Mod | Role::Member | Role::Banned | Role::None
        ),
        Role::Admin => matches!(target_role, Role::Mod | Role::Member | Role::Banned | Role::None),
        Role::Mod => matches!(target_role, Role::Banned | Role::Member | Role::None),
        _ => false,
    }
}

pub async fn community_exists(pool: &PgPool, community_id: &str) -> Result<bool, ApiError> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM communities_local WHERE community_id = $1",
    )
    .bind(community_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;
    Ok(row.is_some())
}
