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
