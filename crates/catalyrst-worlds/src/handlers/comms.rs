use axum::extract::{OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth_chain::{require_verified, AuthChainError};
use crate::http::ApiError;
use crate::livekit::{
    build_adapter_url, world_room_name, world_scene_room_name, AccessToken, VideoGrants,
};
use crate::rate_limiter::RATE_LIMIT_WINDOW_SECONDS;
use crate::AppState;

pub async fn world_comms(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    mint(&state, &world_name, None, uri.path(), &headers).await
}

pub async fn world_scene_comms(
    State(state): State<AppState>,
    Path((world_name, scene_id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    mint(&state, &world_name, Some(&scene_id), uri.path(), &headers).await
}

async fn mint(
    state: &AppState,
    world_name: &str,
    scene_id: Option<&str>,
    path: &str,
    headers: &HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let auth = require_verified(headers, "post", path).map_err(map_auth_error)?;
    let identity = auth.signer.clone();
    let secret = auth.secret();

    let world = state.worlds.get_world(world_name).await?;
    let access = world.as_ref().map(|w| w.access.clone()).unwrap_or_default();
    let owner = world.as_ref().and_then(|w| w.owner.clone());
    let is_shared_secret = access.is_shared_secret();

    let subject = headers
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| identity.clone());

    if is_shared_secret
        && state
            .rate_limiter
            .is_rate_limited(world_name, &subject)
            .await
    {
        return Err(ApiError::too_many(
            "Too many shared-secret attempts. Try again later.",
            RATE_LIMIT_WINDOW_SECONDS,
        ));
    }

    if state.worlds.is_wallet_blocked(&identity).await?
        || state.bans.is_player_banned(&identity).await
    {
        return Err(ApiError::unauthorized(
            "Access denied, you are banned from the platform.",
        ));
    }

    let (room, scene_base): (String, Option<String>) = if let Some(scene_id) = scene_id {
        let base = state
            .worlds
            .get_scene_base_parcel(world_name, scene_id)
            .await?;
        match base {
            Some(base) => (world_scene_room_name(world_name, scene_id), Some(base)),
            None => {
                return Err(ApiError::not_found(format!(
                    "Scene \"{}\" not found in world \"{}\".",
                    scene_id, world_name
                )));
            }
        }
    } else {
        (world_room_name(world_name), None)
    };

    if let Some(base) = scene_base.as_ref() {
        if state
            .bans
            .is_user_banned_from_scene(&identity, world_name, base)
            .await
        {
            return Err(ApiError::unauthorized(format!(
                "You are banned from world \"{}\".",
                world_name
            )));
        }
    }

    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&identity))
        .unwrap_or(false);
    let allowed = is_owner || access.check_access(&identity, secret.as_deref());
    if !allowed {
        if is_shared_secret {
            let tripped = state
                .rate_limiter
                .record_failed_attempt(world_name, &subject)
                .await;
            if tripped {
                return Err(ApiError::too_many(
                    "Too many shared-secret attempts. Try again later.",
                    RATE_LIMIT_WINDOW_SECONDS,
                ));
            }
            return Err(ApiError::forbidden(format!(
                "Access denied, invalid secret for world \"{}\".",
                world_name
            )));
        }
        return Err(ApiError::unauthorized(format!(
            "You are not allowed to access world \"{}\".",
            world_name
        )));
    }

    let participant_count = state
        .presence
        .world_participant_count(&world_name.to_lowercase());
    if participant_count >= state.cfg.max_users_per_world {
        return Err(ApiError::service_unavailable(format!(
            "World \"{}\" has reached its maximum capacity.",
            world_name
        )));
    }

    if is_shared_secret {
        state
            .rate_limiter
            .clear_attempts(world_name, &subject)
            .await;
    }

    let token = AccessToken::new(
        state.cfg.livekit_api_key.clone(),
        state.cfg.livekit_api_secret.clone(),
        identity.clone(),
        VideoGrants::join(room),
    )
    .to_jwt()
    .map_err(|e| ApiError::internal(format!("token mint failed: {e}")))?;

    let fixed_adapter = build_adapter_url(&state.cfg.livekit_ws_url, &token);

    Ok(Json(json!({ "fixedAdapter": fixed_adapter })))
}

fn map_auth_error(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::ForbiddenSigner => ApiError::unauthorized(e.to_string()),
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => ApiError::bad_request(e.to_string()),
        _ => ApiError::unauthorized(e.to_string()),
    }
}
