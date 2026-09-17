use crate::http::{forbidden, ApiError};
use crate::AppState;

pub const PLATFORM_BANNED_MSG: &str = "Access denied, platform-banned user";

/// Upstream answers a connection ban lookup with the device recorded for the
/// address whenever the caller sends none (`getActiveBanForConnection` in
/// logic/user-moderation), so a ban that named a device still catches the same
/// device under a fresh wallet on routes whose signed metadata carries no
/// `deviceIdentifier` and on the bearer-authenticated voice routes that carry no
/// metadata at all. The recorded-device read is supplementary: a failure leaves
/// the address-only match instead of failing the gate.
pub async fn is_connection_banned(
    state: &AppState,
    address: &str,
    device_id: Option<&str>,
) -> Result<bool, ApiError> {
    let address = address.to_lowercase();
    let resolved = match device_id.map(str::trim).filter(|d| !d.is_empty()) {
        Some(device_id) => Some(device_id.to_string()),
        None => match state.player_connection.get_device_id(&address).await {
            Ok(device_id) => device_id,
            Err(error) => {
                tracing::warn!(
                    %error,
                    %address,
                    "failed to resolve the recorded device for a ban lookup"
                );
                None
            }
        },
    };
    state
        .user_bans
        .is_banned_for_connection(&address, resolved.as_deref())
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
    for address in addresses {
        if is_connection_banned(state, address, None).await? {
            tracing::warn!(
                %address,
                "rejected voice credentials for a platform-banned user"
            );
            return Err(forbidden(PLATFORM_BANNED_MSG));
        }
    }
    Ok(())
}
