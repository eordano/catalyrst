use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::json;

use crate::rest::fed::apply;
use crate::rest::fed::authority::{require_min_role, Role};
use crate::rest::fed::ids::{community_id_hex, community_uuid_from_hex};
use crate::rest::fed::messages::{CommunityCreate, CommunityDelete, CommunityUpdate};
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    emit_gossip, err_json, into_resp, map_apply_err, ok_json, ok_json_with, preflight,
    require_owned_name, require_permission, uuid_from_path,
};

pub async fn create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if crate::rest::handlers::client::is_federation_envelope(&body) {
        return into_resp(fed_create_community(State(state), headers, body).await);
    }
    crate::rest::handlers::client::create_community(State(state), headers, body).await
}

async fn fed_create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let (signed, signer) = match preflight::<CommunityCreate>(
        &state,
        &headers,
        "post",
        "/v1/communities",
        &body,
    )
    .await
    {
        Ok(x) => x,
        Err(e) => return e,
    };

    if let Err(e) = crate::rest::validate::validate_name(&signed.message.name) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::rest::validate::validate_description(&signed.message.description) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }

    if let Err(e) = require_owned_name(&state, &signer).await {
        return e;
    }

    let expected_id = community_id_hex(&signer, &signed.message.name, &signed.nonce);

    match apply::apply_create(&state.pool, &signed, &signer).await {
        Ok(out) => {
            emit_gossip(&state, &signed, &out.signature_hash, &signer).await;
            ok_json_with(
                out.signature_hash,
                json!({ "community_id": out.community_id, "id": out.uuid, "expected_id": expected_id }),
            )
        }
        Err(e) => map_apply_err(e),
    }
}

async fn run_community_update(
    state: AppState,
    headers: HeaderMap,
    id: String,
    body: Bytes,
    method: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let (signed, signer) =
        match preflight::<CommunityUpdate>(&state, &headers, method, &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(
            StatusCode::BAD_REQUEST,
            "community_id in body does not match path",
        );
    }
    if let Err(e) = crate::rest::validate::validate_name_opt(signed.message.name.as_deref()) {
        return err_json(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) =
        crate::rest::validate::validate_description_opt(signed.message.description.as_deref())
    {
        return err_json(StatusCode::BAD_REQUEST, e);
    }

    if let Err(e) = require_permission(
        &state,
        &signed.message.community_id,
        &signer,
        Permission::EditInfo,
        "edit the community",
    )
    .await
    {
        return e;
    }
    if signed.message.name.is_some() {
        if let Err(e) = require_permission(
            &state,
            &signed.message.community_id,
            &signer,
            Permission::EditName,
            "edit the community name",
        )
        .await
        {
            return e;
        }
    }
    if signed.message.private.is_some() || signed.message.unlisted.is_some() {
        if let Err(e) = require_permission(
            &state,
            &signed.message.community_id,
            &signer,
            Permission::EditSettings,
            "update the community privacy",
        )
        .await
        {
            return e;
        }
    }
    match apply::apply_update(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}

pub async fn update_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if crate::rest::handlers::client::is_federation_envelope(&body) {
        return into_resp(run_community_update(state, headers, id, body, "put").await);
    }
    crate::rest::handlers::client::update_community(State(state), headers, Path(id), body).await
}

pub async fn update_community_partially(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if crate::rest::handlers::client::is_federation_envelope(&body) {
        return into_resp(run_community_update(state, headers, id, body, "patch").await);
    }
    crate::rest::handlers::client::update_community_partially(State(state), headers, Path(id), body)
        .await
}

pub async fn delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> axum::response::Response {
    if !crate::rest::handlers::client::is_federation_envelope(&body) {
        return crate::rest::handlers::client::delete_community(State(state), headers, Path(id))
            .await;
    }
    into_resp(fed_delete_community(State(state), headers, Path(id), body).await)
}

async fn fed_delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let uuid = match uuid_from_path(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let (signed, signer) =
        match preflight::<CommunityDelete>(&state, &headers, "delete", &path, &body).await {
            Ok(x) => x,
            Err(e) => return e,
        };
    if community_uuid_from_hex(&signed.message.community_id) != uuid {
        return err_json(StatusCode::BAD_REQUEST, "community_id mismatch");
    }
    match require_min_role(
        &state.pool,
        &signed.message.community_id,
        &signer,
        Role::Owner,
    )
    .await
    {
        Ok(_) => {}
        Err(e) => return map_apply_err(e),
    }
    match apply::apply_delete(&state.pool, &signed).await {
        Ok(sig) => {
            emit_gossip(&state, &signed, &sig, &signer).await;
            ok_json(sig)
        }
        Err(e) => map_apply_err(e),
    }
}
