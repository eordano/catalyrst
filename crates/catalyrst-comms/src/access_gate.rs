use crate::http::{forbidden, ApiError};
use crate::AppState;

pub const PLATFORM_BANNED_MSG: &str = "Access denied, platform-banned user";

/// Upstream answers a connection ban lookup with the device recorded for the
/// address whenever the caller sends none (`getActiveBanForConnection` in
/// logic/user-moderation), so a ban that named a device still catches the same
/// device under a fresh wallet on routes whose signed metadata carries no
/// `deviceIdentifier` and on the bearer-authenticated voice routes that carry no
/// metadata at all. One statement resolves both; a fault fails the gate closed.
pub async fn is_connection_banned(
    state: &AppState,
    address: &str,
    device_id: Option<&str>,
) -> Result<bool, ApiError> {
    state
        .user_bans
        .is_banned_for_connection(address, device_id)
        .await
}

/// Upstream refuses voice credentials to any platform-banned participant before
/// it mints a token (`assertNoActivePlatformBan` in logic/voice), so neither a
/// community role nor the other side of a private call outranks a platform ban.
/// Either side of a private call refuses it: the other would be left alone in a
/// dead room.
pub async fn ensure_no_active_platform_ban(
    state: &AppState,
    addresses: &[&str],
) -> Result<(), ApiError> {
    if let Some(idx) = state
        .user_bans
        .first_banned_for_connection(addresses)
        .await?
    {
        let address = addresses[idx];
        tracing::warn!(
            %address,
            "rejected voice credentials for a platform-banned user"
        );
        return Err(forbidden(PLATFORM_BANNED_MSG));
    }
    Ok(())
}
