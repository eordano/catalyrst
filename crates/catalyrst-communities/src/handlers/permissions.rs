//! Upstream permission model (social-service-ea `src/logic/community/roles.ts`).
//!
//! Exactly three real roles — owner, moderator, member — gated by a static
//! permission matrix rather than an ordinal hierarchy. `none`/`banned` carry no
//! permissions. This mirrors `OWNER_PERMISSIONS`, `MODERATOR_PERMISSIONS`,
//! `hasPermission`, `canActOnMember` and `ROLE_ACTION_TRANSITIONS`.

use crate::fed::authority::Role;

/// Discrete community permissions, matching upstream `CommunityPermission`.
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

fn role_permissions(role: Role) -> &'static [Permission] {
    match role {
        Role::Owner => OWNER_PERMISSIONS,
        Role::Mod => MODERATOR_PERMISSIONS,
        // Member, None and Banned carry no permissions of their own.
        Role::Member | Role::None | Role::Banned => &[],
    }
}

/// `hasPermission(role, permission)` — upstream roles.ts.
pub fn has_permission(role: Role, permission: Permission) -> bool {
    role_permissions(role).contains(&permission)
}

/// `isMember(role)` — any real role other than `none`/`banned`.
pub fn is_member(role: Role) -> bool {
    !matches!(role, Role::None | Role::Banned)
}

/// `validatePermissionToCreatePost` — `create_posts` is owner/moderator only.
pub fn can_create_post(role: Role) -> bool {
    has_permission(role, Permission::CreatePosts)
}

/// `validatePermissionToDeletePost` — the deleter needs `delete_posts`
/// (owner/moderator; a plain-member author CANNOT delete their own post), and a
/// moderator may delete only their OWN posts (owners delete any).
pub fn can_delete_post(role: Role, is_author: bool) -> bool {
    has_permission(role, Permission::DeletePosts) && (role != Role::Mod || is_author)
}

/// `validatePermissionsToLikeAndUnlikePost` — any non-banned user may like in a
/// PUBLIC community (role `none` included); in a PRIVATE community the signer
/// must be a member.
pub fn can_like_post(role: Role, community_is_private: bool) -> bool {
    role != Role::Banned && !(community_is_private && role == Role::None)
}

/// `canActOnMember(actorRole, targetRole)` — upstream `ROLE_ACTION_TRANSITIONS`.
///
/// - No one can act on an owner.
/// - Only owners can act on moderators.
/// - Owners and moderators can act on members.
pub fn can_act_on_member(actor: Role, target: Role) -> bool {
    match target {
        Role::Owner => false,
        Role::Mod => actor == Role::Owner,
        Role::Member => matches!(actor, Role::Owner | Role::Mod),
        Role::None | Role::Banned => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Value-parity with social-service-ea roles.ts: OWNER has all 15
    // permissions; MODERATOR has the 11-permission subset (no edit_name,
    // edit_settings, delete_community, assign_roles); member/none/banned none.

    #[test]
    fn owner_holds_all_permissions() {
        for p in OWNER_PERMISSIONS {
            assert!(has_permission(Role::Owner, *p), "owner missing {:?}", p);
        }
        assert_eq!(OWNER_PERMISSIONS.len(), 15);
    }

    #[test]
    fn moderator_matrix_matches_upstream() {
        assert_eq!(MODERATOR_PERMISSIONS.len(), 11);
        // Held by moderators.
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
            assert!(has_permission(Role::Mod, p), "mod missing {:?}", p);
        }
        // Owner-only — moderators must NOT hold these.
        for p in [
            Permission::EditName,
            Permission::EditSettings,
            Permission::DeleteCommunity,
            Permission::AssignRoles,
        ] {
            assert!(!has_permission(Role::Mod, p), "mod wrongly has {:?}", p);
        }
    }

    #[test]
    fn member_and_below_have_no_permissions() {
        for role in [Role::Member, Role::None, Role::Banned] {
            assert!(!has_permission(role, Permission::EditInfo));
            assert!(!has_permission(role, Permission::CreatePosts));
            assert!(!has_permission(role, Permission::BanPlayers));
        }
    }

    #[test]
    fn is_member_excludes_none_and_banned() {
        assert!(is_member(Role::Owner));
        assert!(is_member(Role::Mod));
        assert!(is_member(Role::Member));
        assert!(!is_member(Role::None));
        assert!(!is_member(Role::Banned));
    }

    // Post-gate parity with posts.ts / roles.ts.

    #[test]
    fn create_post_is_owner_and_moderator_only() {
        // `create_posts` ∈ owner/moderator; plain members do NOT have it.
        assert!(can_create_post(Role::Owner));
        assert!(can_create_post(Role::Mod));
        assert!(!can_create_post(Role::Member), "member cannot create posts");
        assert!(!can_create_post(Role::None));
        assert!(!can_create_post(Role::Banned));
    }

    #[test]
    fn delete_post_owner_any_mod_own_member_never() {
        // Owner deletes ANY post (own or others').
        assert!(can_delete_post(Role::Owner, true));
        assert!(can_delete_post(Role::Owner, false));
        // Moderator deletes only their OWN post.
        assert!(can_delete_post(Role::Mod, true));
        assert!(!can_delete_post(Role::Mod, false));
        // A plain-member author CANNOT delete even their own post.
        assert!(!can_delete_post(Role::Member, true));
        assert!(!can_delete_post(Role::Member, false));
        assert!(!can_delete_post(Role::None, true));
        assert!(!can_delete_post(Role::Banned, true));
    }

    #[test]
    fn like_post_public_open_private_members_only() {
        // PUBLIC community: any non-banned user, role `none` included.
        assert!(can_like_post(Role::None, false));
        assert!(can_like_post(Role::Member, false));
        assert!(can_like_post(Role::Mod, false));
        assert!(can_like_post(Role::Owner, false));
        // PRIVATE community: a non-member (role `none`) is denied; members allowed.
        assert!(!can_like_post(Role::None, true), "non-member denied in private");
        assert!(can_like_post(Role::Member, true));
        assert!(can_like_post(Role::Mod, true));
        assert!(can_like_post(Role::Owner, true));
        // Banned is always denied.
        assert!(!can_like_post(Role::Banned, false));
        assert!(!can_like_post(Role::Banned, true));
    }

    #[test]
    fn role_action_transitions_match_upstream() {
        // No one can act on an owner.
        for actor in [Role::Owner, Role::Mod, Role::Member, Role::None] {
            assert!(!can_act_on_member(actor, Role::Owner));
        }
        // Only owners act on moderators.
        assert!(can_act_on_member(Role::Owner, Role::Mod));
        assert!(!can_act_on_member(Role::Mod, Role::Mod));
        assert!(!can_act_on_member(Role::Member, Role::Mod));
        // Owners and moderators act on members.
        assert!(can_act_on_member(Role::Owner, Role::Member));
        assert!(can_act_on_member(Role::Mod, Role::Member));
        assert!(!can_act_on_member(Role::Member, Role::Member));
        // None target: nobody.
        assert!(!can_act_on_member(Role::Owner, Role::None));
    }
}
