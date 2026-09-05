use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::handlers::scene_adapter::fetch_world_scene_id;
use crate::http::ApiError;
use crate::AppState;

pub const WORLD_BAN_STATUS_PATH: &str =
    "/worlds/{world_name}/parcels/{base_parcel}/users/{address}/ban-status";

/// Ban status for `address` at a world parcel, for the worlds content server.
///
/// # Fail-closed contract
///
/// This handler answers `200 {"isBanned": true}` -- not a 5xx -- whenever it
/// cannot establish that the user is *un*banned. That looks like it throws away
/// the error, so it is worth spelling out why propagating would be worse.
///
/// The consumer is [`catalyrst_worlds`'s `BansComponent::is_user_banned_from_scene`]
/// (`crates/catalyrst-worlds/src/ports/bans.rs`), which collapses every non-2xx
/// response *and* every transport error to `false` = not banned. So an `Err`
/// returned from here does not deny anything: it round-trips into exactly the
/// silent un-ban this handler is meant to prevent. The response body is the only
/// channel in this contract that the caller actually reads, so denying has to be
/// expressed in the body.
///
/// The tradeoff is deliberate: while the worlds content server or the ban store
/// is unreachable, world scene access is denied for everyone rather than granted
/// to banned users. An outage-shaped blanket deny is loud and gets fixed; a
/// silent un-ban is neither.
///
/// The unresolvable-world branch cannot legitimately fire for a live
/// world: the caller only asks about a parcel after it has already resolved a
/// deployed scene at that parcel from its own store, so a `None` here means the
/// content server is unreachable or inconsistent, not that the world is empty.
pub async fn world_ban_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((world_name, base_parcel, address)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::moderator::require_service_token(&state, &headers)?;

    let Some(place_id) = fetch_world_scene_id(&state, &world_name).await else {
        tracing::error!(
            world = %world_name,
            parcel = %base_parcel,
            %address,
            "world ban check: could not resolve scene id; failing closed as banned"
        );
        return Ok(Json(serde_json::json!({ "isBanned": true })));
    };

    let is_banned = match state.scene_bans.is_banned(&place_id, &address).await {
        Ok(banned) => banned,
        Err(e) => {
            tracing::error!(
                error = %e,
                world = %world_name,
                parcel = %base_parcel,
                %address,
                %place_id,
                "world ban check: ban store query failed; failing closed as banned"
            );
            return Ok(Json(serde_json::json!({ "isBanned": true })));
        }
    };
    tracing::debug!(
        world = %world_name,
        parcel = %base_parcel,
        %address,
        is_banned,
        "world ban check"
    );
    Ok(Json(serde_json::json!({ "isBanned": is_banned })))
}

#[cfg(test)]
fn path_params(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{path_params, WORLD_BAN_STATUS_PATH};

    #[test]
    fn world_ban_status_route_includes_base_parcel_in_order() {
        let params = path_params(WORLD_BAN_STATUS_PATH);
        assert_eq!(
            params.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["world_name", "base_parcel", "address"]
        );
    }

    #[test]
    fn world_ban_status_path_matches_worlds_content_server_client_shape() {
        let filled = WORLD_BAN_STATUS_PATH
            .replace("{world_name}", "foo.eth")
            .replace("{base_parcel}", "0,0")
            .replace("{address}", "0xabc");
        assert_eq!(filled, "/worlds/foo.eth/parcels/0,0/users/0xabc/ban-status");
    }
}
