use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;

use crate::rest::community_membership_authority::{
    load_standing_from_community_role_current, CommunityMembershipTier,
};
use crate::rest::fed::apply;
use crate::rest::fed::ids::community_uuid_from_hex;
use crate::rest::fed::messages::{CommunityPlaceRemove, CommunityPlacesAdd};
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, map_refusal_using_its_own_detail, ok_json,
    preflight, require_permission, require_places_ownership, uuid_from_path,
};

#[utoipa::path(
    post,
    path = "/v1/communities/{id}/places",
    tag = "places",
    params(("id" = String, Path)),
    request_body(content = serde_json::Value, description = "{ placeIds } for client, or EIP-712 federation envelope"),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 502, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
        signer.as_str(),
        Permission::AddPlaces,
        "add places to the community",
    )
    .await
    {
        return e;
    }
    if let Err(e) =
        require_places_ownership(&state, &signed.message.place_ids, signer.as_str()).await
    {
        return e;
    }
    match apply::apply_places_add(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
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

#[utoipa::path(
    delete,
    path = "/v1/communities/{id}/places/{placeId}",
    tag = "places",
    params(("id" = String, Path), ("placeId" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 409, body = catalyrst_types::ApiErrorBody),
        (status = 429, body = catalyrst_types::ApiErrorBody),
        (status = 502, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
        signer.as_str(),
    )
    .await
    {
        return e;
    }
    let actor_standing = match load_standing_from_community_role_current(
        &state.pool,
        &signed.message.community_id,
        signer.as_str(),
    )
    .await
    {
        Ok(s) => s,
        Err(refusal) => return map_refusal_using_its_own_detail(&refusal, StatusCode::FORBIDDEN),
    };
    if actor_standing.tier() != CommunityMembershipTier::OwnerOfThisCommunity {
        if let Err(e) = require_permission(
            &state,
            &signed.message.community_id,
            signer.as_str(),
            Permission::RemovePlaces,
            "remove places from the community",
        )
        .await
        {
            return e;
        }
    }
    match apply::apply_place_remove(&state.pool, &signed, signer.as_str()).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, signer.as_str()).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}
