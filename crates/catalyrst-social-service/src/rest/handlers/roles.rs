pub fn has_moderation_permission(role: Option<&str>) -> bool {
    matches!(
        role,
        Some("owner") | Some("moderator") | Some("mod") | Some("admin")
    )
}
