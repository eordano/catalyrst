use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use sqlx::Row;
use uuid::Uuid;

use crate::auth_chain::verify_signed_fetch;
use crate::handlers::responses::SceneStreamAccessResponse;
use crate::http::{auth_error, forbidden, not_implemented, service_unavailable, ApiError};
use crate::livekit::{scene_room_name, world_scene_room_name, IngressClient};
use crate::AppState;

use super::scene_adapter::{fetch_world_scene_id, meta_str};

const SCENE_SIGNER: &str = "decentraland-kernel-scene";
const FOUR_DAYS_MS: i64 = 4 * 24 * 60 * 60 * 1000;

pub const STREAM_ACCESS_NOT_FOUND_MSG: &str = "No active streaming access found for place";
pub const STREAM_RESET_FAILED_MSG: &str = "Failed to reset scene stream access";

pub async fn cast_any() -> Response {
    not_implemented(
        "Cast 2.0 WebRTC presenter surface (stream links, streamer/watcher/bot tokens, presenter \
         promote/demote) is not yet ported; RTMP scene-stream ingress is served by \
         /scene-stream-access. See TODO.md",
    )
}

// Handles both PUT (get-or-create an RTMP ingress for the caller's scene) and DELETE (revoke it),
// matching comms-gatekeeper's scene-stream-access. Fails closed when LiveKit ingress credentials
// are absent so a missing config can never masquerade as a granted stream key.
pub async fn scene_stream_access(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    _body: Bytes,
) -> Result<Response, ApiError> {
    if !state.livekit_configured {
        return Err(service_unavailable(
            "LiveKit ingress is not configured (set LIVEKIT_API_KEY / LIVEKIT_API_SECRET)",
        ));
    }

    let verb = method.as_str().to_lowercase();
    let sf = verify_signed_fetch(&headers, &verb, "/scene-stream-access", &[SCENE_SIGNER])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;

    let realm_name = meta_str(&sf.metadata, "realmName")
        .or_else(|| {
            sf.metadata
                .get("realm")
                .and_then(|r| meta_str(r, "serverName"))
        })
        .ok_or_else(|| ApiError::bad_request("invalid signed-fetch request, no realmName"))?;
    let parcel = meta_str(&sf.metadata, "parcel");
    let raw_scene_id = meta_str(&sf.metadata, "sceneId")
        .ok_or_else(|| ApiError::bad_request("invalid signed-fetch request, no sceneId"))?;
    let is_world = realm_name.ends_with(".eth");

    let scene_id = if is_world && raw_scene_id.ends_with(".eth") {
        fetch_world_scene_id(&state, &realm_name)
            .await
            .ok_or_else(|| {
                ApiError::bad_request(format!("failed to resolve scene ID for world {realm_name}"))
            })?
    } else {
        raw_scene_id
    };

    let place_id = resolve_place_id(&state, is_world, &realm_name, parcel.as_deref())
        .await?
        .ok_or_else(|| ApiError::not_found("place not found for this scene"))?;

    if !crate::scene_perms::is_scene_owner_or_admin(&state, &place_id, sf.signer.as_str()).await? {
        return Err(forbidden("you are not authorized to stream to this scene"));
    }

    let room = if is_world {
        world_scene_room_name(&realm_name, &scene_id)
    } else {
        scene_room_name(&realm_name, &scene_id)
    };

    let ingress = IngressClient::new(
        &state.http,
        &state.livekit_host,
        &state.livekit_api_key,
        &state.livekit_api_secret,
    );

    match method {
        Method::GET => match fetch_active_access(&state, &place_id).await? {
            Some(mut resp) => {
                resp.ends_at = resp.created_at + FOUR_DAYS_MS;
                Ok(axum::Json(resp).into_response())
            }
            None => Err(ApiError::not_found(STREAM_ACCESS_NOT_FOUND_MSG)),
        },
        Method::POST => add_access(&state, &ingress, &place_id, &room, sf.signer.as_str()).await,
        Method::PUT => reset_access(&state, &ingress, &place_id, &room, sf.signer.as_str()).await,
        Method::DELETE => remove_access(&state, &ingress, &place_id).await,
        _ => Ok(StatusCode::METHOD_NOT_ALLOWED.into_response()),
    }
}

async fn resolve_place_id(
    state: &AppState,
    is_world: bool,
    realm_name: &str,
    parcel: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(pool) = state.places_pool.as_ref() else {
        tracing::warn!("scene-stream-access: places pool unavailable; cannot resolve place");
        return Ok(None);
    };
    let row = if is_world {
        sqlx::query(
            "SELECT id FROM place \
             WHERE COALESCE((raw->>'world')::bool, false) = true \
               AND lower(raw->>'world_name') = lower($1) LIMIT 1",
        )
        .bind(realm_name)
        .fetch_optional(pool)
        .await?
    } else {
        let Some(parcel) = parcel else {
            return Err(ApiError::bad_request(
                "invalid signed-fetch request, no parcel",
            ));
        };
        sqlx::query(
            "SELECT id FROM place \
             WHERE base_position = $1 OR raw->'positions' @> to_jsonb($1::text) LIMIT 1",
        )
        .bind(parcel)
        .fetch_optional(pool)
        .await?
    };
    Ok(row.and_then(|r| r.try_get::<String, _>("id").ok()))
}

async fn add_access(
    state: &AppState,
    ingress: &IngressClient<'_>,
    place_id: &str,
    room: &str,
    signer: &str,
) -> Result<Response, ApiError> {
    if let Some(existing) = fetch_active_access(state, place_id).await? {
        return Ok(axum::Json(existing).into_response());
    }
    create_access(state, ingress, place_id, room, signer).await
}

async fn create_access(
    state: &AppState,
    ingress: &IngressClient<'_>,
    place_id: &str,
    room: &str,
    signer: &str,
) -> Result<Response, ApiError> {
    let participant_identity = format!("{}-streamer", Uuid::new_v4());
    let info = ingress
        .get_or_create_ingress(room, &participant_identity)
        .await
        .map_err(|e| ApiError::internal(format!("livekit ingress create: {e}")))?;

    let (Some(url), Some(stream_key)) = (info.url.as_deref(), info.stream_key.as_deref()) else {
        return Err(ApiError::internal(
            "livekit ingress response missing url or stream key",
        ));
    };

    let row = sqlx::query(
        "INSERT INTO scene_stream_access \
            (place_id, streaming_url, streaming_key, ingress_id, room_id, generated_by, \
             expiration_time, active) \
         VALUES ($1, $2, $3, $4, $5, $6, now() + ($7 || ' milliseconds')::interval, true) \
         RETURNING (extract(epoch from created_at) * 1000)::bigint AS created_ms, \
                   (extract(epoch from expiration_time) * 1000)::bigint AS ends_ms",
    )
    .bind(place_id)
    .bind(url)
    .bind(stream_key)
    .bind(&info.ingress_id)
    .bind(room)
    .bind(signer.to_lowercase())
    .bind(FOUR_DAYS_MS.to_string())
    .fetch_one(&state.pool)
    .await?;

    let created_ms: i64 = row.try_get("created_ms").unwrap_or(0);
    let ends_ms: i64 = row
        .try_get::<Option<i64>, _>("ends_ms")
        .ok()
        .flatten()
        .unwrap_or(created_ms + FOUR_DAYS_MS);

    Ok(axum::Json(SceneStreamAccessResponse {
        streaming_url: url.to_string(),
        streaming_key: stream_key.to_string(),
        created_at: created_ms,
        ends_at: ends_ms,
    })
    .into_response())
}

async fn reset_access(
    state: &AppState,
    ingress: &IngressClient<'_>,
    place_id: &str,
    room: &str,
    signer: &str,
) -> Result<Response, ApiError> {
    let row = sqlx::query(
        "SELECT ingress_id FROM scene_stream_access \
         WHERE place_id = $1 AND active = true LIMIT 1",
    )
    .bind(place_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| ApiError::http(500, STREAM_RESET_FAILED_MSG))?;

    let Some(row) = row else {
        return Err(ApiError::http(500, STREAM_RESET_FAILED_MSG));
    };

    if let Some(ingress_id) = row
        .try_get::<Option<String>, _>("ingress_id")
        .ok()
        .flatten()
    {
        if !ingress_id.is_empty() {
            ingress
                .delete_ingress(&ingress_id)
                .await
                .map_err(|_| ApiError::http(500, STREAM_RESET_FAILED_MSG))?;
        }
    }

    sqlx::query(
        "UPDATE scene_stream_access SET active = false WHERE place_id = $1 AND active = true",
    )
    .bind(place_id)
    .execute(&state.pool)
    .await
    .map_err(|_| ApiError::http(500, STREAM_RESET_FAILED_MSG))?;

    create_access(state, ingress, place_id, room, signer)
        .await
        .map_err(|_| ApiError::http(500, STREAM_RESET_FAILED_MSG))
}

async fn remove_access(
    state: &AppState,
    ingress: &IngressClient<'_>,
    place_id: &str,
) -> Result<Response, ApiError> {
    let row = sqlx::query(
        "SELECT ingress_id FROM scene_stream_access \
         WHERE place_id = $1 AND active = true LIMIT 1",
    )
    .bind(place_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found(
            "no active stream access for this scene",
        ));
    };

    if let Some(ingress_id) = row
        .try_get::<Option<String>, _>("ingress_id")
        .ok()
        .flatten()
    {
        if !ingress_id.is_empty() {
            ingress
                .delete_ingress(&ingress_id)
                .await
                .map_err(|e| ApiError::internal(format!("livekit ingress delete: {e}")))?;
        }
    }

    sqlx::query(
        "UPDATE scene_stream_access SET active = false WHERE place_id = $1 AND active = true",
    )
    .bind(place_id)
    .execute(&state.pool)
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn fetch_active_access(
    state: &AppState,
    place_id: &str,
) -> Result<Option<SceneStreamAccessResponse>, ApiError> {
    let row = sqlx::query(
        "SELECT streaming_url, streaming_key, \
                (extract(epoch from created_at) * 1000)::bigint AS created_ms, \
                (extract(epoch from expiration_time) * 1000)::bigint AS ends_ms \
         FROM scene_stream_access WHERE place_id = $1 AND active = true LIMIT 1",
    )
    .bind(place_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let url: String = row.try_get("streaming_url").unwrap_or_default();
    let key: String = row.try_get("streaming_key").unwrap_or_default();
    let created_ms: i64 = row.try_get("created_ms").unwrap_or(0);
    let ends_ms: i64 = row
        .try_get::<Option<i64>, _>("ends_ms")
        .ok()
        .flatten()
        .unwrap_or(created_ms + FOUR_DAYS_MS);
    Ok(Some(SceneStreamAccessResponse {
        streaming_url: url,
        streaming_key: key,
        created_at: created_ms,
        ends_at: ends_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_body_matches_upstream_shape() {
        let v = serde_json::to_value(SceneStreamAccessResponse {
            streaming_url: "rtmp://ingest/x".into(),
            streaming_key: "sk_abc".into(),
            created_at: 1_700_000_000_000,
            ends_at: 1_700_345_600_000,
        })
        .unwrap();
        assert_eq!(v["streaming_url"], "rtmp://ingest/x");
        assert_eq!(v["streaming_key"], "sk_abc");
        assert_eq!(v["created_at"], 1_700_000_000_000i64);
        assert_eq!(v["ends_at"], 1_700_345_600_000i64);
    }

    #[test]
    fn four_days_window_is_ms() {
        assert_eq!(FOUR_DAYS_MS, 345_600_000);
    }
}
