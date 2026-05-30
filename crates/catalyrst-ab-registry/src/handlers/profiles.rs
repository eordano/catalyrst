use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::{ApiError, ApiResult};
use crate::ports::content::ActiveEntity;
use crate::types::{CompactProfile, IdsBody, MAX_POINTERS};
use crate::AppState;

pub async fn post_profiles(
    State(state): State<AppState>,
    Json(body): Json<IdsBody>,
) -> ApiResult<Json<Vec<Value>>> {
    let ids = validate_ids(body.ids)?;
    let profiles = state.content.resolve_profiles(&ids).await?;
    let base = &state.profile_images_url;
    Ok(Json(profiles.into_iter().map(|e| sanitized_profile(&e, base)).collect()))
}

pub async fn post_profiles_metadata(
    State(state): State<AppState>,
    Json(body): Json<IdsBody>,
) -> ApiResult<Json<Vec<CompactProfile>>> {
    let ids = validate_ids(body.ids)?;
    let profiles = state.content.resolve_profiles(&ids).await?;
    let base = &state.profile_images_url;
    Ok(Json(profiles.iter().map(|e| compact_profile(e, base)).collect()))
}

fn validate_ids(ids: Vec<String>) -> Result<Vec<String>, ApiError> {
    if ids.is_empty() {
        return Err(ApiError::bad_request("ids must be a non-empty array"));
    }
    if ids.len() > MAX_POINTERS {
        return Err(ApiError::bad_request(format!(
            "too many ids: {} (max {})",
            ids.len(),
            MAX_POINTERS
        )));
    }
    Ok(ids)
}

fn face_url(base: &str, entity_id: &str) -> String {
    format!("{base}/entities/{entity_id}/face.png")
}

fn body_url(base: &str, entity_id: &str) -> String {
    format!("{base}/entities/{entity_id}/body.png")
}

fn first_avatar(metadata: &Value) -> Option<&Value> {
    metadata.get("avatars").and_then(|a| a.as_array()).and_then(|a| a.first())
}

fn sanitized_profile(ent: &ActiveEntity, base: &str) -> Value {
    let face = face_url(base, &ent.entity_id);
    let body = body_url(base, &ent.entity_id);

    let avatars: Vec<Value> = ent
        .metadata
        .get("avatars")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .map(|avatar| rewrite_avatar(avatar, &face, &body))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "timestamp": ent.timestamp,
        "avatars": avatars,
    })
}

fn rewrite_avatar(avatar: &Value, face: &str, body: &str) -> Value {
    let mut avatar = avatar.clone();
    if let Some(obj) = avatar.as_object_mut() {
        if let Some(inner) = obj.get_mut("avatar").and_then(|a| a.as_object_mut()) {
            inner.insert(
                "snapshots".to_string(),
                json!({ "face256": face, "body": body }),
            );
        }
    }
    avatar
}

fn compact_profile(ent: &ActiveEntity, base: &str) -> CompactProfile {
    let pointer = ent.pointers.first().cloned().unwrap_or_default();
    let avatar = first_avatar(&ent.metadata);

    let name = avatar
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();

    let has_claimed_name = avatar
        .and_then(|a| a.get("hasClaimedName"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let name_color = avatar
        .and_then(|a| a.get("nameColor"))
        .filter(|v| !v.is_null())
        .cloned();

    CompactProfile {
        pointer,
        has_claimed_name,
        name,
        name_color,

        thumbnail_url: face_url(base, &ent.entity_id),
    }
}
