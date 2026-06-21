use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::http::ApiError;
use crate::livekit::{
    address_from_identity, list_room_participant_identities, scene_room_name, world_room_name,
    world_scene_room_name,
};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ParticipantsQuery {
    pub pointer: Option<String>,
    pub realm_name: Option<String>,
    pub room: Option<String>,
}

fn is_world_name(name: &str) -> bool {
    name.ends_with(".eth")
}

fn empty_roster() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "data": { "addresses": Vec::<String>::new() }
    }))
}

async fn resolve_scene_id(state: &AppState, pointer: &str) -> Option<String> {
    let url = format!("{}/content/entities/active", state.catalyst_url);
    let resp = state
        .http
        .post(&url)
        .json(&serde_json::json!({ "pointers": [pointer] }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let entities: serde_json::Value = resp.json().await.ok()?;
    entities
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.get("id"))
        .and_then(|id| id.as_str())
        .map(String::from)
}

pub async fn list_participants(
    State(state): State<AppState>,
    Query(q): Query<ParticipantsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pointer = q.pointer.as_deref().filter(|s| !s.is_empty());
    let mut realm_name = q.realm_name.or(q.room).filter(|s| !s.is_empty());

    if pointer.is_some() && realm_name.is_none() {
        realm_name = Some("main".to_string());
    }

    if pointer.is_none() && realm_name.is_none() {
        return Err(ApiError::bad_request(
            "Either pointer or realm_name must be provided",
        ));
    }

    let realm = realm_name.as_deref().unwrap_or("main");

    let room_name = if is_world_name(realm) {
        match pointer {
            Some(p) => match resolve_scene_id(&state, p).await {
                Some(scene_id) => world_scene_room_name(realm, &scene_id),
                None => return Ok(empty_roster()),
            },
            None => world_room_name(realm),
        }
    } else {
        let Some(p) = pointer else {
            return Err(ApiError::bad_request(
                "Either pointer with realm_name or a world realm_name must be provided",
            ));
        };
        match resolve_scene_id(&state, p).await {
            Some(scene_id) => scene_room_name(realm, &scene_id),
            None => return Ok(empty_roster()),
        }
    };

    let identities = list_room_participant_identities(
        &state.http,
        &state.livekit_host,
        &state.livekit_api_key,
        &state.livekit_api_secret,
        &room_name,
    )
    .await;

    let addresses: Vec<String> = identities
        .iter()
        .filter_map(|id| address_from_identity(id))
        .collect();

    Ok(Json(serde_json::json!({
        "ok": true,
        "data": { "addresses": addresses }
    })))
}
