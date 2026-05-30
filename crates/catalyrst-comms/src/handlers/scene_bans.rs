use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, ApiError};
use crate::AppState;

const SCENE_SIGNER: &str = "decentraland-kernel-scene";

#[derive(Debug, Deserialize)]
pub struct BanQuery {
    pub place_id: Option<String>,
    pub banned_address: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BanBody {
    pub place_id: String,
    pub banned_address: String,
}

const MAX_LIMIT: i64 = 100;

fn pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64, i64) {
    let limit = match limit {
        Some(l) if l > 0 && l <= MAX_LIMIT => l,
        _ => MAX_LIMIT,
    };
    let offset = offset.filter(|o| *o >= 0).unwrap_or(0);
    let page = (offset / limit) + 1;
    (limit, offset, page.max(1))
}

fn pages(total: i64, limit: i64) -> i64 {
    if limit <= 0 {
        0
    } else {
        (total + limit - 1) / limit
    }
}

pub async fn list_bans(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-bans", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .or_else(|| super::scene_adapter::place_from_metadata(&sf.metadata))
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;

    let (limit, offset, page) = pagination(q.limit, q.offset);
    let total = state.scene_bans.count(&place_id).await?;
    let addresses = state.scene_bans.list_addresses_page(&place_id, limit, offset).await?;
    let names = state.names.get_names_from_addresses(&addresses).await;

    let results: Vec<serde_json::Value> = addresses
        .into_iter()
        .map(|addr| {
            let name = names.get(&addr).cloned().unwrap_or_default();
            serde_json::json!({ "bannedAddress": addr, "name": name })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "results": results,
        "total": total,
        "page": page,
        "pages": pages(total, limit),
        "limit": limit,
    })))
}

pub async fn list_ban_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-bans/addresses", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .or_else(|| super::scene_adapter::place_from_metadata(&sf.metadata))
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;

    let (limit, offset, page) = pagination(q.limit, q.offset);
    let total = state.scene_bans.count(&place_id).await?;
    let addresses = state.scene_bans.list_addresses_page(&place_id, limit, offset).await?;

    Ok(Json(serde_json::json!({
        "results": addresses,
        "total": total,
        "page": page,
        "pages": pages(total, limit),
        "limit": limit,
    })))
}

pub async fn ban_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BanBody>,
) -> Result<impl IntoResponse, ApiError> {
    let sf = verify_signed_fetch(&headers, "post", "/scene-bans", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    if !crate::scene_perms::is_scene_owner_or_admin(&state, &body.place_id, &sf.signer).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    state.scene_bans.ban(&body.place_id, &body.banned_address, &sf.signer).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unban_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let sf = verify_signed_fetch(&headers, "delete", "/scene-bans", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;
    let banned_address = q
        .banned_address
        .ok_or_else(|| ApiError::bad_request("missing banned_address query"))?;
    if !crate::scene_perms::is_scene_owner_or_admin(&state, &place_id, &sf.signer).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    state.scene_bans.unban(&place_id, &banned_address).await?;
    Ok(StatusCode::NO_CONTENT)
}
