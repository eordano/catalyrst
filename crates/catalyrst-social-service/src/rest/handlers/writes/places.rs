use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;

use crate::rest::fed::apply;
use crate::rest::fed::authority::{load_role, Role};
use crate::rest::fed::ids::community_uuid_from_hex;
use crate::rest::fed::messages::{CommunityPlaceRemove, CommunityPlacesAdd};
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, ok_json, preflight, require_permission,
    require_places_ownership, uuid_from_path,
};

pub async fn add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::add_places(State(state), headers, Path(id), body)
            .await;
    }
    into_resp(fed_add_places(State(state), headers, Path(id), body).await)
}

async fn fed_add_places(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places", id);
    let (signed, signer) =
        match preflight::<CommunityPlacesAdd>(&state, &headers, "post", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }

    if let Err(e) = require_permission(
        &state,
        &signed.message.community_id,
        &signer,
        Permission::AddPlaces,
        "add places to the community",
    )
    .await
    {
        return e;
    }
    if let Err(e) = require_places_ownership(&state, &signed.message.place_ids, &signer).await {
        return e;
    }
    match apply::apply_places_add(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
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
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::remove_place(
            State(state),
            headers,
            Path(crate::rest::handlers::client::PathIdPlace { id, place_id }),
        )
        .await;
    }
    into_resp(
        fed_remove_place(
            State(state),
            headers,
            Path(PathIdPlace { id, place_id }),
            body,
        )
        .await,
    )
}

async fn fed_remove_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(PathIdPlace { id, place_id }): Path<PathIdPlace>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}/places/{}", id, place_id);
    let (signed, signer) =
        match preflight::<CommunityPlaceRemove>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    if signed.message.place_id != place_id {
        return err_json(StatusCode::BAD_REQUEST, "place_id mismatch");
    }

    if let Err(e) = require_places_ownership(
        &state,
        std::slice::from_ref(&signed.message.place_id),
        &signer,
    )
    .await
    {
        return e;
    }
    let actor_role = match load_role(&state.pool, &signed.message.community_id, &signer).await {
        Ok(r) => r,
        Err(e) => return map_apply_err(e),
    };
    if actor_role != Role::Owner {
        if let Err(e) = require_permission(
            &state,
            &signed.message.community_id,
            &signer,
            Permission::RemovePlaces,
            "remove places from the community",
        )
        .await
        {
            return e;
        }
    }
    match apply::apply_place_remove(&state.pool, &signed, &signer).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}
