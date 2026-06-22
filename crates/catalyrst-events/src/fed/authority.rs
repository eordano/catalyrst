//! Moderator authority for events federation actions (events.md §3).
//!
//! Upstream gates profile-settings + schedule writes on the actor's
//! `profile_settings.permissions` (EditAnyProfile / EditAnySchedule). The
//! federation port collapses that to a single local moderator allow-list: a
//! wallet present in the `moderators` table may sign profile-settings actions on
//! other users and schedule upserts. Self-edits of `me/settings` need no
//! moderator status.

use sqlx::PgPool;

use crate::http::response::ApiError;

pub async fn is_moderator(pool: &PgPool, address: &str) -> Result<bool, ApiError> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM moderators WHERE address = $1")
        .bind(address.to_ascii_lowercase())
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn require_moderator(pool: &PgPool, signer: &str) -> Result<(), ApiError> {
    if is_moderator(pool, signer).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden("Forbidden"))
    }
}

/// Whether a profile-settings write is allowed: self-edits (`target == signer`)
/// are open; editing another user's settings requires moderator status. Pure so
/// the policy is testable without a DB (the `is_moderator` lookup is the only
/// I/O, performed by the caller).
pub fn settings_write_allowed(signer: &str, target: &str, signer_is_moderator: bool) -> bool {
    target.eq_ignore_ascii_case(signer) || signer_is_moderator
}

#[cfg(test)]
mod tests {
    use super::settings_write_allowed;

    #[test]
    fn self_edit_needs_no_moderator() {
        assert!(settings_write_allowed("0xAbC", "0xabc", false));
    }

    #[test]
    fn editing_another_user_requires_moderator() {
        assert!(!settings_write_allowed("0x1", "0x2", false));
        assert!(settings_write_allowed("0x1", "0x2", true));
    }
}
