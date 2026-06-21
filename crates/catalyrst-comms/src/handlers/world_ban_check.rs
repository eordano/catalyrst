use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::handlers::scene_adapter::fetch_world_scene_id;
use crate::http::{unauthorized, ApiError};
use crate::AppState;

pub const WORLD_BAN_STATUS_PATH: &str =
    "/worlds/{world_name}/parcels/{base_parcel}/users/{address}/ban-status";

pub async fn world_ban_check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((world_name, base_parcel, address)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(expected) = state.gatekeeper_auth_token.as_deref() {
        let ok = crate::moderator::bearer_token(&headers)
            .map(|t| crate::moderator::timing_safe_eq(&t, expected))
            .unwrap_or(false);
        if !ok {
            return Err(unauthorized("Invalid authorization header"));
        }
    }

    let Some(place_id) = fetch_world_scene_id(&state, &world_name).await else {
        tracing::warn!(
            world = %world_name,
            parcel = %base_parcel,
            "world ban check: could not resolve scene id; treating as not banned"
        );
        return Ok(Json(serde_json::json!({ "isBanned": false })));
    };

    let is_banned = state
        .scene_bans
        .is_banned(&place_id, &address)
        .await
        .unwrap_or(false);
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
