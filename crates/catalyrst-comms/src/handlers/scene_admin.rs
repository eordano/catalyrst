use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, ApiError};
use crate::AppState;

const SCENE_SIGNER: &str = "decentraland-kernel-scene";
const SERVER_SIGNER: &str = "dcl:authoritative-server";

#[derive(Debug, Deserialize)]
pub struct PlaceQuery {
    pub place_id: Option<String>,
    pub admin: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddAdminBody {
    pub place_id: String,
    pub admin: String,
}

pub async fn list_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PlaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-admin", &[SCENE_SIGNER, SERVER_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .or_else(|| super::scene_adapter::place_from_metadata(&sf.metadata))
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;

    let mut addresses = state.scene_admin.list_admin_addresses(&place_id).await?;

    if let Some(filter) = q.admin.as_deref() {
        let filter = filter.to_lowercase();
        addresses.retain(|a| a.eq_ignore_ascii_case(&filter));
    }

    let names = state.names.get_names_from_addresses(&addresses).await;

    let body: Vec<serde_json::Value> = addresses
        .into_iter()
        .map(|admin| {
            let name = names.get(&admin).cloned().unwrap_or_default();
            serde_json::json!({
                "admin": admin,
                "name": name,
                "canBeRemoved": true,
            })
        })
        .collect();

    Ok(Json(serde_json::Value::Array(body)))
}

pub async fn add_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddAdminBody>,
) -> Result<impl IntoResponse, ApiError> {
    let sf = verify_signed_fetch(&headers, "post", "/scene-admin", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    if !crate::scene_perms::is_scene_owner_or_admin(&state, &body.place_id, &sf.signer).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    state.scene_admin.add(&body.place_id, &body.admin, &sf.signer).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PlaceQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let sf = verify_signed_fetch(&headers, "delete", "/scene-admin", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;
    let admin = q
        .admin
        .ok_or_else(|| ApiError::bad_request("missing admin query"))?;
    if !crate::scene_perms::is_scene_owner_or_admin(&state, &place_id, &sf.signer).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    state.scene_admin.remove(&place_id, &admin).await?;
    Ok(StatusCode::NO_CONTENT)
}
