//! The capability matrix for one wallet acting inside one community.
//!
//! The 15/11 matrix below is unchanged by the typed-authority migration; only the type of
//! the tier parameter moved, from `crate::rest::fed::authority::Role` to
//! [`CommunityMembershipTier`].
//!
//! # On the name `Permission`
//!
//! It stays. Within this module it is unambiguous, and the workspace-wide rule this
//! migration follows -- never reuse a short noun across two authorities -- is satisfied by
//! never re-exporting it: `catalyrst-worlds` calls its ACL name a `permission: &str` and
//! `catalyrst-land-authz` has `ParcelPermissionFlags`, and neither may ever meet this
//! one. The three-way collision is recorded in the naming table; the resolution here is
//! containment rather than a rename, because the matrix and its test are the best-pinned
//! code in this crate and a rename would churn them for no behavioural gain.

use crate::rest::community_membership_authority::CommunityMembershipTier;

/// One capability a community membership tier may hold **within its own community**.
///
/// Not a world ACL name, not a LAND grant bitmask, not a platform-wide power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    EditInfo,
    EditName,
    AddPlaces,
    RemovePlaces,
    AcceptRequests,
    RejectRequests,
    ViewRequests,
    BanPlayers,
    SendInvitations,
    EditSettings,
    DeleteCommunity,
    AssignRoles,
    InviteUsers,
    CreatePosts,
    DeletePosts,
}

const OWNER_PERMISSIONS: &[Permission] = &[
    Permission::EditInfo,
    Permission::EditName,
    Permission::AddPlaces,
    Permission::RemovePlaces,
    Permission::AcceptRequests,
    Permission::RejectRequests,
    Permission::ViewRequests,
    Permission::BanPlayers,
    Permission::SendInvitations,
    Permission::EditSettings,
    Permission::DeleteCommunity,
    Permission::AssignRoles,
    Permission::InviteUsers,
    Permission::CreatePosts,
    Permission::DeletePosts,
];

const MODERATOR_PERMISSIONS: &[Permission] = &[
    Permission::EditInfo,
    Permission::AddPlaces,
    Permission::RemovePlaces,
    Permission::AcceptRequests,
    Permission::RejectRequests,
    Permission::ViewRequests,
    Permission::BanPlayers,
    Permission::SendInvitations,
    Permission::InviteUsers,
    Permission::CreatePosts,
    Permission::DeletePosts,
];

fn role_permissions(tier: CommunityMembershipTier) -> &'static [Permission] {
    match tier {
        CommunityMembershipTier::OwnerOfThisCommunity => OWNER_PERMISSIONS,
        CommunityMembershipTier::ModeratorOfThisCommunity => MODERATOR_PERMISSIONS,

        CommunityMembershipTier::OrdinaryMemberOfThisCommunity
        | CommunityMembershipTier::NotAMemberOfThisCommunity
        | CommunityMembershipTier::BannedFromThisCommunity => &[],
    }
}

pub fn has_permission(tier: CommunityMembershipTier, permission: Permission) -> bool {
    role_permissions(tier).contains(&permission)
}

pub fn is_member(tier: CommunityMembershipTier) -> bool {
    !matches!(
        tier,
        CommunityMembershipTier::NotAMemberOfThisCommunity
            | CommunityMembershipTier::BannedFromThisCommunity
    )
}

pub fn can_create_post(tier: CommunityMembershipTier) -> bool {
    has_permission(tier, Permission::CreatePosts)
}

/// Owner may delete any post; a moderator may delete only their own.
///
/// The `tier != Moderator || is_author` shape reads like an inverted condition and is
/// **not** being changed by this migration. If it is a bug it is a separate finding with
/// its own commit; a refactor that compiles but flips a predicate is worse than no
/// refactor.
pub fn can_delete_post(tier: CommunityMembershipTier, is_author: bool) -> bool {
    has_permission(tier, Permission::DeletePosts)
        && (tier != CommunityMembershipTier::ModeratorOfThisCommunity || is_author)
}

pub fn can_like_post(tier: CommunityMembershipTier, community_is_private: bool) -> bool {
    tier != CommunityMembershipTier::BannedFromThisCommunity
        && !(community_is_private && tier == CommunityMembershipTier::NotAMemberOfThisCommunity)
}

/// Whether an actor's tier outranks a target's tier for member-directed actions.
///
/// This is one half of the shared ban/unban predicate; the other half is
/// `has_permission(actor, BanPlayers)`. Both halves are combined in exactly one place,
/// [`crate::rest::community_membership_authority::ban_authority`].
pub fn can_act_on_member(actor: CommunityMembershipTier, target: CommunityMembershipTier) -> bool {
    match target {
        CommunityMembershipTier::OwnerOfThisCommunity => false,
        CommunityMembershipTier::ModeratorOfThisCommunity => {
            actor == CommunityMembershipTier::OwnerOfThisCommunity
        }
        CommunityMembershipTier::OrdinaryMemberOfThisCommunity => {
            matches!(
                actor,
                CommunityMembershipTier::OwnerOfThisCommunity
                    | CommunityMembershipTier::ModeratorOfThisCommunity
            )
        }
        CommunityMembershipTier::NotAMemberOfThisCommunity
        | CommunityMembershipTier::BannedFromThisCommunity => false,
    }
}

#[cfg(test)]
mod tests {
    use super::CommunityMembershipTier as Tier;
    use super::*;

    #[test]
    fn owner_holds_all_permissions() {
        for p in OWNER_PERMISSIONS {
            assert!(
                has_permission(Tier::OwnerOfThisCommunity, *p),
                "owner missing {:?}",
                p
            );
        }
        assert_eq!(OWNER_PERMISSIONS.len(), 15);
    }

    #[test]
    fn moderator_matrix_matches_upstream() {
        assert_eq!(MODERATOR_PERMISSIONS.len(), 11);

        for p in [
            Permission::EditInfo,
            Permission::AddPlaces,
            Permission::RemovePlaces,
            Permission::AcceptRequests,
            Permission::RejectRequests,
            Permission::ViewRequests,
            Permission::BanPlayers,
            Permission::SendInvitations,
            Permission::InviteUsers,
            Permission::CreatePosts,
            Permission::DeletePosts,
        ] {
            assert!(
                has_permission(Tier::ModeratorOfThisCommunity, p),
                "mod missing {:?}",
                p
            );
        }

        for p in [
            Permission::EditName,
            Permission::EditSettings,
            Permission::DeleteCommunity,
            Permission::AssignRoles,
        ] {
            assert!(
                !has_permission(Tier::ModeratorOfThisCommunity, p),
                "mod wrongly has {:?}",
                p
            );
        }
    }

    #[test]
    fn member_and_below_have_no_permissions() {
        for tier in [
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::NotAMemberOfThisCommunity,
            Tier::BannedFromThisCommunity,
        ] {
            assert!(!has_permission(tier, Permission::EditInfo));
            assert!(!has_permission(tier, Permission::CreatePosts));
            assert!(!has_permission(tier, Permission::BanPlayers));
        }
    }

    #[test]
    fn is_member_excludes_none_and_banned() {
        assert!(is_member(Tier::OwnerOfThisCommunity));
        assert!(is_member(Tier::ModeratorOfThisCommunity));
        assert!(is_member(Tier::OrdinaryMemberOfThisCommunity));
        assert!(!is_member(Tier::NotAMemberOfThisCommunity));
        assert!(!is_member(Tier::BannedFromThisCommunity));
    }

    #[test]
    fn create_post_is_owner_and_moderator_only() {
        assert!(can_create_post(Tier::OwnerOfThisCommunity));
        assert!(can_create_post(Tier::ModeratorOfThisCommunity));
        assert!(
            !can_create_post(Tier::OrdinaryMemberOfThisCommunity),
            "member cannot create posts"
        );
        assert!(!can_create_post(Tier::NotAMemberOfThisCommunity));
        assert!(!can_create_post(Tier::BannedFromThisCommunity));
    }

    #[test]
    fn delete_post_owner_any_mod_own_member_never() {
        assert!(can_delete_post(Tier::OwnerOfThisCommunity, true));
        assert!(can_delete_post(Tier::OwnerOfThisCommunity, false));

        assert!(can_delete_post(Tier::ModeratorOfThisCommunity, true));
        assert!(!can_delete_post(Tier::ModeratorOfThisCommunity, false));

        assert!(!can_delete_post(Tier::OrdinaryMemberOfThisCommunity, true));
        assert!(!can_delete_post(Tier::OrdinaryMemberOfThisCommunity, false));
        assert!(!can_delete_post(Tier::NotAMemberOfThisCommunity, true));
        assert!(!can_delete_post(Tier::BannedFromThisCommunity, true));
    }

    #[test]
    fn like_post_public_open_private_members_only() {
        assert!(can_like_post(Tier::NotAMemberOfThisCommunity, false));
        assert!(can_like_post(Tier::OrdinaryMemberOfThisCommunity, false));
        assert!(can_like_post(Tier::ModeratorOfThisCommunity, false));
        assert!(can_like_post(Tier::OwnerOfThisCommunity, false));

        assert!(
            !can_like_post(Tier::NotAMemberOfThisCommunity, true),
            "non-member denied in private"
        );
        assert!(can_like_post(Tier::OrdinaryMemberOfThisCommunity, true));
        assert!(can_like_post(Tier::ModeratorOfThisCommunity, true));
        assert!(can_like_post(Tier::OwnerOfThisCommunity, true));

        assert!(!can_like_post(Tier::BannedFromThisCommunity, false));
        assert!(!can_like_post(Tier::BannedFromThisCommunity, true));
    }

    #[test]
    fn role_action_transitions_match_upstream() {
        for actor in [
            Tier::OwnerOfThisCommunity,
            Tier::ModeratorOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::NotAMemberOfThisCommunity,
        ] {
            assert!(!can_act_on_member(actor, Tier::OwnerOfThisCommunity));
        }

        assert!(can_act_on_member(
            Tier::OwnerOfThisCommunity,
            Tier::ModeratorOfThisCommunity
        ));
        assert!(!can_act_on_member(
            Tier::ModeratorOfThisCommunity,
            Tier::ModeratorOfThisCommunity
        ));
        assert!(!can_act_on_member(
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::ModeratorOfThisCommunity
        ));

        assert!(can_act_on_member(
            Tier::OwnerOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity
        ));
        assert!(can_act_on_member(
            Tier::ModeratorOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity
        ));
        assert!(!can_act_on_member(
            Tier::OrdinaryMemberOfThisCommunity,
            Tier::OrdinaryMemberOfThisCommunity
        ));

        assert!(!can_act_on_member(
            Tier::OwnerOfThisCommunity,
            Tier::NotAMemberOfThisCommunity
        ));
    }
}
