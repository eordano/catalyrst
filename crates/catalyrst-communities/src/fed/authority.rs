use sqlx::PgPool;

use crate::http::ApiError;

/// Upstream community role set (`social-service-ea` `CommunityRole`): exactly
/// owner / moderator / member, plus the `none`/`banned` sentinels. There is no
/// `admin` tier — authorization is the static permission matrix in
/// `handlers::permissions`, not this ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Banned,
    None,
    Member,
    Mod,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
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

pub async fn community_exists(pool: &PgPool, community_id: &str) -> Result<bool, ApiError> {
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT 1 FROM communities_local WHERE community_id = $1")
            .bind(community_id)
            .fetch_optional(pool)
            .await
            .map_err(ApiError::from)?;
    Ok(row.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::permissions::{has_permission, Permission};

    /// Upstream `CommunityRole` has exactly owner/moderator/member — no `admin`.
    /// The legacy `"admin"` string must not parse to any role (it is not a real
    /// upstream tier), and no role must ever render as `"admin"`.
    #[test]
    fn admin_tier_is_removed() {
        assert_eq!(Role::parse("admin"), None);
        for role in [
            Role::Owner,
            Role::Mod,
            Role::Member,
            Role::Banned,
            Role::None,
        ] {
            assert_ne!(role.as_str(), "admin");
        }
    }

    /// Round-trip parity for the three real roles + sentinels with upstream's
    /// `CommunityRole` strings (moderator with the `mod` alias).
    #[test]
    fn role_string_round_trip() {
        assert_eq!(Role::parse("owner"), Some(Role::Owner));
        assert_eq!(Role::parse("moderator"), Some(Role::Mod));
        assert_eq!(Role::parse("mod"), Some(Role::Mod));
        assert_eq!(Role::parse("member"), Some(Role::Member));
        assert_eq!(Role::parse("banned"), Some(Role::Banned));
        assert_eq!(Role::parse("none"), Some(Role::None));
        assert_eq!(Role::parse(""), Some(Role::None));
        assert_eq!(Role::Owner.as_str(), "owner");
        assert_eq!(Role::Member.as_str(), "member");
    }

    /// The per-operation gates wired into the federation write handlers must map
    /// to upstream's `roles.ts` matrix exactly (no `admin` tier in between):
    ///   - `edit_info`, `add_places`, `remove_places` → owner + moderator;
    ///   - `edit_name`, `edit_settings` → owner only.
    #[test]
    fn write_path_permission_gates_match_upstream_matrix() {
        // Shared owner/moderator operations.
        for p in [
            Permission::EditInfo,
            Permission::AddPlaces,
            Permission::RemovePlaces,
        ] {
            assert!(has_permission(Role::Owner, p), "owner missing {:?}", p);
            assert!(has_permission(Role::Mod, p), "mod missing {:?}", p);
            assert!(!has_permission(Role::Member, p));
            assert!(!has_permission(Role::None, p));
            assert!(!has_permission(Role::Banned, p));
        }
        // Owner-only settings/name operations.
        for p in [Permission::EditName, Permission::EditSettings] {
            assert!(has_permission(Role::Owner, p), "owner missing {:?}", p);
            assert!(!has_permission(Role::Mod, p), "mod wrongly has {:?}", p);
            assert!(!has_permission(Role::Member, p));
        }
    }
}
