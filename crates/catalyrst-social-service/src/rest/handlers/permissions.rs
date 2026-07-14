use crate::rest::fed::authority::Role;

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

        Role::Member | Role::None | Role::Banned => &[],
    }
}

pub fn has_permission(role: Role, permission: Permission) -> bool {
    role_permissions(role).contains(&permission)
}

pub fn is_member(role: Role) -> bool {
    !matches!(role, Role::None | Role::Banned)
}

pub fn can_create_post(role: Role) -> bool {
    has_permission(role, Permission::CreatePosts)
}

pub fn can_delete_post(role: Role, is_author: bool) -> bool {
    has_permission(role, Permission::DeletePosts) && (role != Role::Mod || is_author)
}

pub fn can_like_post(role: Role, community_is_private: bool) -> bool {
    role != Role::Banned && !(community_is_private && role == Role::None)
}

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

    #[test]
    fn create_post_is_owner_and_moderator_only() {
        assert!(can_create_post(Role::Owner));
        assert!(can_create_post(Role::Mod));
        assert!(!can_create_post(Role::Member), "member cannot create posts");
        assert!(!can_create_post(Role::None));
        assert!(!can_create_post(Role::Banned));
    }

    #[test]
    fn delete_post_owner_any_mod_own_member_never() {
        assert!(can_delete_post(Role::Owner, true));
        assert!(can_delete_post(Role::Owner, false));

        assert!(can_delete_post(Role::Mod, true));
        assert!(!can_delete_post(Role::Mod, false));

        assert!(!can_delete_post(Role::Member, true));
        assert!(!can_delete_post(Role::Member, false));
        assert!(!can_delete_post(Role::None, true));
        assert!(!can_delete_post(Role::Banned, true));
    }

    #[test]
    fn like_post_public_open_private_members_only() {
        assert!(can_like_post(Role::None, false));
        assert!(can_like_post(Role::Member, false));
        assert!(can_like_post(Role::Mod, false));
        assert!(can_like_post(Role::Owner, false));

        assert!(
            !can_like_post(Role::None, true),
            "non-member denied in private"
        );
        assert!(can_like_post(Role::Member, true));
        assert!(can_like_post(Role::Mod, true));
        assert!(can_like_post(Role::Owner, true));

        assert!(!can_like_post(Role::Banned, false));
        assert!(!can_like_post(Role::Banned, true));
    }

    #[test]
    fn role_action_transitions_match_upstream() {
        for actor in [Role::Owner, Role::Mod, Role::Member, Role::None] {
            assert!(!can_act_on_member(actor, Role::Owner));
        }

        assert!(can_act_on_member(Role::Owner, Role::Mod));
        assert!(!can_act_on_member(Role::Mod, Role::Mod));
        assert!(!can_act_on_member(Role::Member, Role::Mod));

        assert!(can_act_on_member(Role::Owner, Role::Member));
        assert!(can_act_on_member(Role::Mod, Role::Member));
        assert!(!can_act_on_member(Role::Member, Role::Member));

        assert!(!can_act_on_member(Role::Owner, Role::None));
    }
}
