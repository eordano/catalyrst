use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, ApiError};
use crate::ports::extra_addresses;
use crate::AppState;

use super::scene_adapter::{fetch_world_scene_id, meta_str};

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

fn listing_key_candidate(meta: &Value) -> Option<String> {
    let realm_name = meta_str(meta, "realmName")
        .or_else(|| meta.get("realm").and_then(|r| meta_str(r, "serverName")));
    let scene_id = meta_str(meta, "sceneId");
    match realm_name {
        Some(realm) if realm.ends_with(".eth") => match scene_id {
            Some(id) if !id.ends_with(".eth") => Some(id),
            _ => Some(realm),
        },
        _ => scene_id,
    }
}

pub async fn resolve_listing_place_id(
    state: &AppState,
    explicit: Option<String>,
    meta: &Value,
) -> Result<String, ApiError> {
    let candidate = explicit
        .filter(|s| !s.is_empty())
        .or_else(|| listing_key_candidate(meta))
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;
    if !candidate.ends_with(".eth") {
        return Ok(candidate);
    }
    fetch_world_scene_id(state, &candidate)
        .await
        .ok_or_else(|| {
            ApiError::bad_request(format!("Failed to resolve scene ID for world {candidate}"))
        })
}

pub async fn list_bans(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-bans", &[SCENE_SIGNER])
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = resolve_listing_place_id(&state, q.place_id, &sf.metadata).await?;

    let (limit, offset, page) = pagination(q.limit, q.offset);
    let total = state.scene_bans.count(&place_id).await?;
    let addresses = state
        .scene_bans
        .list_addresses_page(&place_id, limit, offset)
        .await?;
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
    let place_id = resolve_listing_place_id(&state, q.place_id, &sf.metadata).await?;

    let (limit, offset, page) = pagination(q.limit, q.offset);
    let total = state.scene_bans.count(&place_id).await?;
    let addresses = state
        .scene_bans
        .list_addresses_page(&place_id, limit, offset)
        .await?;

    Ok(Json(serde_json::json!({
        "results": addresses,
        "total": total,
        "page": page,
        "pages": pages(total, limit),
        "limit": limit,
    })))
}

pub async fn ensure_target_not_protected(
    state: &AppState,
    place_id: &str,
    target: &str,
) -> Result<(), ApiError> {
    let target = target.to_lowercase();
    if crate::scene_perms::is_scene_owner_or_admin(state, place_id, &target).await? {
        return Err(ApiError::bad_request("Cannot ban this address"));
    }
    if let Some(place) = extra_addresses::load_place_info(state, place_id).await {
        let mut protected = extra_addresses::get_extra_addresses(state, &place).await;
        if !place.world {
            protected.extend(
                extra_addresses::get_lease_holders_for_parcels(state, &place.positions).await,
            );
        }
        if protected.contains(&target) {
            return Err(ApiError::bad_request("Cannot ban this address"));
        }
    }
    Ok(())
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
    ensure_target_not_protected(&state, &body.place_id, &body.banned_address).await?;
    state
        .scene_bans
        .ban(&body.place_id, &body.banned_address, &sf.signer)
        .await?;
    crate::room_metadata_sync::add_ban(&state, &body.place_id, &body.banned_address).await;
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
    crate::room_metadata_sync::remove_ban(&state, &place_id, &banned_address).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::listing_key_candidate;
    use serde_json::json;

    #[test]
    fn world_realm_with_scene_hash_keys_on_the_hash() {
        let meta = json!({ "realmName": "foo.dcl.eth", "sceneId": "bafkreiabc" });
        assert_eq!(listing_key_candidate(&meta).as_deref(), Some("bafkreiabc"));
    }

    #[test]
    fn world_realm_with_eth_scene_id_falls_back_to_world_name() {
        let meta = json!({ "realmName": "foo.dcl.eth", "sceneId": "foo.dcl.eth" });
        assert_eq!(listing_key_candidate(&meta).as_deref(), Some("foo.dcl.eth"));
    }

    #[test]
    fn world_realm_without_scene_id_falls_back_to_world_name() {
        let meta = json!({ "realmName": "foo.dcl.eth" });
        assert_eq!(listing_key_candidate(&meta).as_deref(), Some("foo.dcl.eth"));
        let nested = json!({ "realm": { "serverName": "bar.eth" } });
        assert_eq!(listing_key_candidate(&nested).as_deref(), Some("bar.eth"));
    }

    #[test]
    fn genesis_realm_keys_on_scene_id() {
        let meta = json!({ "realmName": "main", "sceneId": "bafkreixyz" });
        assert_eq!(listing_key_candidate(&meta).as_deref(), Some("bafkreixyz"));
    }

    #[test]
    fn missing_metadata_yields_none() {
        assert_eq!(listing_key_candidate(&json!({})), None);
        assert_eq!(listing_key_candidate(&json!({ "realmName": "main" })), None);
    }
}
