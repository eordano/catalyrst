use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::rest::fed::authority::Role;
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    auth, err, load_role_uuid, map_db, parse_uuid, require_permission_uuid,
    validate_places_ownership,
};

#[derive(Debug, Deserialize)]
pub struct PlacesBody {
    #[serde(rename = "placeIds", default)]
    pub place_ids: Vec<String>,
}

pub async fn add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places", id);
    let signer = match auth(&headers, "post", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::AddPlaces,
        "add places to the community",
    )
    .await
    {
        return e;
    }
    let parsed: PlacesBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid body: {}", e)),
    };
    if parsed.place_ids.is_empty() {
        return err(StatusCode::BAD_REQUEST, "placeIds is required");
    }

    if let Err(e) = validate_places_ownership(&state, &parsed.place_ids, &signer).await {
        return e;
    }
    for pid in &parsed.place_ids {
        let ins = sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(uuid)
        .bind(&signer)
        .execute(&state.pool)
        .await;
        if let Err(e) = map_db(ins) {
            return e;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct PathIdPlace {
    pub id: String,
    #[serde(rename = "placeId")]
    pub place_id: String,
}

pub async fn remove_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPlace { id, place_id }): Path<PathIdPlace>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places/{}", id, place_id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) =
        validate_places_ownership(&state, std::slice::from_ref(&place_id), &signer).await
    {
        return e;
    }
    let member_role = load_role_uuid(&state, uuid, &signer).await;
    if member_role != Role::Owner {
        if let Err(e) = require_permission_uuid(
            &state,
            uuid,
            &signer,
            Permission::RemovePlaces,
            "remove places from the community",
        )
        .await
        {
            return e;
        }
    }
    let del = sqlx::query("DELETE FROM community_places WHERE id = $1 AND community_id = $2")
        .bind(&place_id)
        .bind(uuid)
        .execute(&state.pool)
        .await;
    if let Err(e) = map_db(del) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}
