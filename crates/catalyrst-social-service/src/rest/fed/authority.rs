//! The federation path's view of community authority.
//!
//! The tier vocabulary that used to live here as `Role` now lives in
//! [`crate::rest::community_membership_authority`], shared with the client path. What
//! remains here is the federation-specific write gate and two community lookups.

use sqlx::PgPool;

use catalyrst_authenticated_principal::AuthorityNotEstablished;

use crate::rest::community_membership_authority::{
    load_standing_from_community_role_current, CommunityMembershipStanding, CommunityMembershipTier,
};
use crate::rest::http::ApiError;

/// The right of a federated originating wallet to write to one community, proven against
/// the **`community_role_current`** table.
///
/// This is not the client path's authority: that one is proven against `community_members`
/// and is named `ClientCommunityWriteAuthority` in
/// [`crate::rest::handlers::client`]. The two tables can disagree, and the whole point of
/// giving them different type names is that a reviewer can see which one decided.
///
/// Replaces the old `require_min_role`, message for message and status for status.
#[derive(Debug)]
pub struct FederatedCommunityWriteAuthority {
    standing: CommunityMembershipStanding,
}

impl FederatedCommunityWriteAuthority {
    /// Behaviour-preserving replacement for `require_min_role(pool, community, signer, min)`.
    ///
    /// Order and wording of the two refusals are unchanged: an explicitly banned wallet is
    /// refused first and by name, then a wallet below the minimum tier is refused with its
    /// own tier and the required one spelled out. A query failure is
    /// [`AuthorityNotEstablished::UndeterminedStoreUnavailable`], which
    /// is what the old code expressed by propagating `ApiError::Database`.
    pub async fn resolve_requiring_at_least(
        pool: &PgPool,
        community_id_hex_text: &str,
        originating_wallet_address: &str,
        minimum: CommunityMembershipTier,
    ) -> Result<Self, AuthorityNotEstablished> {
        let standing = load_standing_from_community_role_current(
            pool,
            community_id_hex_text,
            originating_wallet_address,
        )
        .await?;
        if standing.tier() == CommunityMembershipTier::BannedFromThisCommunity {
            return Err(AuthorityNotEstablished::RefusedLacksAuthority {
                detail: "Forbidden: banned from this community".to_string(),
            });
        }
        if !standing.tier_is_at_least(minimum) {
            return Err(AuthorityNotEstablished::RefusedLacksAuthority {
                detail: format!(
                    "Forbidden: signer role {} below required {}",
                    standing.tier().as_canonical_stored_role_text(),
                    minimum.as_canonical_stored_role_text()
                ),
            });
        }
        Ok(Self { standing })
    }

    /// The tier this authority was proven at.
    pub fn tier(&self) -> CommunityMembershipTier {
        self.standing.tier()
    }

    /// The full standing, including the table that answered.
    pub fn standing(&self) -> &CommunityMembershipStanding {
        &self.standing
    }
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

pub async fn community_is_private(
    pool: &PgPool,
    community_id: &str,
) -> Result<Option<bool>, ApiError> {
    let uuid = crate::rest::fed::ids::community_uuid_from_hex(community_id);
    let row: Option<(bool,)> = sqlx::query_as("SELECT private FROM communities WHERE id = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::from)?;
    Ok(row.map(|(p,)| p))
}

#[cfg(test)]
mod tests {
    use crate::rest::community_membership_authority::CommunityMembershipTier as Tier;
    use crate::rest::handlers::permissions::{has_permission, Permission};

    /// Preserved verbatim from the old `Role::parse` test. The `"admin"` tier was removed
    /// from the CHECK constraints by `migrations/0006_role_check_reconcile.sql` and must
    /// stay unparseable.
    #[test]
    fn admin_tier_is_removed() {
        assert_eq!(Tier::parse_role_text_supplied_in_a_request("admin"), None);
        for tier in [
            Tier::OwnerOfThisCommunity,
            Tier::ModeratorOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::BannedFromThisCommunity,
            Tier::NotAMemberOfThisCommunity,
        ] {
            assert_ne!(tier.as_canonical_stored_role_text(), "admin");
        }
    }

    #[test]
    fn role_string_round_trip() {
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("owner"),
            Some(Tier::OwnerOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("moderator"),
            Some(Tier::ModeratorOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("mod"),
            Some(Tier::ModeratorOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("member"),
            Some(Tier::OrdinaryMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("banned"),
            Some(Tier::BannedFromThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request("none"),
            Some(Tier::NotAMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::parse_role_text_supplied_in_a_request(""),
            Some(Tier::NotAMemberOfThisCommunity)
        );
        assert_eq!(
            Tier::OwnerOfThisCommunity.as_canonical_stored_role_text(),
            "owner"
        );
        assert_eq!(
            Tier::OrdinaryMemberOfThisCommunity.as_canonical_stored_role_text(),
            "member"
        );
    }

    #[test]
    fn write_path_permission_gates_match_upstream_matrix() {
        for p in [
            Permission::EditInfo,
            Permission::AddPlaces,
            Permission::RemovePlaces,
        ] {
            assert!(
                has_permission(Tier::OwnerOfThisCommunity, p),
                "owner missing {:?}",
                p
            );
            assert!(
                has_permission(Tier::ModeratorOfThisCommunity, p),
                "mod missing {:?}",
                p
            );
            assert!(!has_permission(Tier::OrdinaryMemberOfThisCommunity, p));
            assert!(!has_permission(Tier::NotAMemberOfThisCommunity, p));
            assert!(!has_permission(Tier::BannedFromThisCommunity, p));
        }

        for p in [Permission::EditName, Permission::EditSettings] {
            assert!(
                has_permission(Tier::OwnerOfThisCommunity, p),
                "owner missing {:?}",
                p
            );
            assert!(
                !has_permission(Tier::ModeratorOfThisCommunity, p),
                "mod wrongly has {:?}",
                p
            );
            assert!(!has_permission(Tier::OrdinaryMemberOfThisCommunity, p));
        }
    }
}
