use std::collections::BTreeSet;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use catalyrst_types::{limit_or_max, PaginatedResponse};
use serde::Deserialize;
use serde_json::Value;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, service_unavailable, ApiError};
use crate::livekit::is_world_realm_name;
use crate::ports::extra_addresses::{self, PlaceLookup};
use crate::room_metadata_sync::{self, RoomContext, PLACE_LOOKUP_UNAVAILABLE_MSG};
use crate::AppState;

use super::scene_adapter::{fetch_world_scene_id, meta_str, realm_name_from_metadata};

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

fn pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit_or_max(limit, MAX_LIMIT),
        offset.filter(|o| *o >= 0).unwrap_or(0),
    )
}

fn one_based_page<T>(results: Vec<T>, total: i64, limit: i64, offset: i64) -> PaginatedResponse<T> {
    let mut page = PaginatedResponse::new(results, total, limit, offset);
    page.page += 1;
    page
}

fn listing_key_candidate(meta: &Value) -> Option<String> {
    let realm_name = realm_name_from_metadata(meta);
    let scene_id = meta_str(meta, "sceneId");
    match realm_name {
        Some(realm) if is_world_realm_name(&realm) => match scene_id {
            Some(id) if !is_world_realm_name(&id) => Some(id),
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
    if !is_world_realm_name(&candidate) {
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
) -> Result<Json<PaginatedResponse<serde_json::Value>>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-bans", &[SCENE_SIGNER])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = resolve_listing_place_id(&state, q.place_id, &sf.metadata).await?;

    let (limit, offset) = pagination(q.limit, q.offset);
    let (addresses, total) = state
        .scene_bans
        .list_addresses_page_with_total(&place_id, limit, offset)
        .await?;
    let names = state.names.get_names_from_addresses(&addresses).await;

    let results: Vec<serde_json::Value> = addresses
        .into_iter()
        .map(|addr| {
            let name = names.get(&addr).cloned().unwrap_or_default();
            serde_json::json!({ "bannedAddress": addr, "name": name })
        })
        .collect();

    Ok(Json(one_based_page(results, total, limit, offset)))
}

pub async fn list_ban_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<Json<PaginatedResponse<String>>, ApiError> {
    let sf = verify_signed_fetch(&headers, "get", "/scene-bans/addresses", &[SCENE_SIGNER])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = resolve_listing_place_id(&state, q.place_id, &sf.metadata).await?;

    let (limit, offset) = pagination(q.limit, q.offset);
    let (addresses, total) = state
        .scene_bans
        .list_addresses_page_with_total(&place_id, limit, offset)
        .await?;

    Ok(Json(one_based_page(addresses, total, limit, offset)))
}

pub async fn ensure_target_not_protected(
    state: &AppState,
    place_id: &str,
    target: &str,
) -> Result<(), ApiError> {
    ensure_target_not_protected_for(state, &PlaceLookup::new(state, place_id), target).await
}

/// Same check over a place row shared with the caller's authz step; the two
/// permission surfaces are fetched together.
pub async fn ensure_target_not_protected_for(
    state: &AppState,
    place: &PlaceLookup<'_>,
    target: &str,
) -> Result<(), ApiError> {
    let target = target.to_lowercase();
    if crate::scene_perms::is_scene_owner_or_admin_for(state, place, &target).await? {
        return Err(ApiError::bad_request("Cannot ban this address"));
    }
    let place_id = place.place_id();
    let place = place.get().await.map_err(|error| {
        tracing::error!(
            error,
            %place_id,
            "places lookup failed while checking a ban target for protection"
        );
        service_unavailable(PLACE_LOOKUP_UNAVAILABLE_MSG)
    })?;
    if let Some(place) = place {
        let (extra, leases) = tokio::join!(
            extra_addresses::try_get_extra_addresses(state, place),
            async {
                if place.world {
                    BTreeSet::new()
                } else {
                    extra_addresses::get_lease_holders_for_parcels(state, &place.positions).await
                }
            }
        );
        let mut protected = extra?;
        protected.extend(leases);
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
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let place = PlaceLookup::new(&state, &body.place_id);
    if !crate::scene_perms::is_scene_owner_or_admin_for(&state, &place, sf.signer.as_str()).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    ensure_target_not_protected_for(&state, &place, &body.banned_address).await?;
    let ctx = RoomContext::from_metadata(&sf.metadata, &body.place_id);
    let rooms = room_metadata_sync::resolve_rooms_for(&state, &ctx, &place).await?;
    state
        .scene_bans
        .ban(&body.place_id, &body.banned_address, sf.signer.as_str())
        .await?;
    tokio::join!(
        room_metadata_sync::kick(&state, &rooms, &body.banned_address),
        room_metadata_sync::add_ban(&state, &rooms, &body.banned_address),
    );
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unban_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BanQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let sf = verify_signed_fetch(&headers, "delete", "/scene-bans", &[SCENE_SIGNER])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;
    let banned_address = q
        .banned_address
        .ok_or_else(|| ApiError::bad_request("missing banned_address query"))?;
    let place = PlaceLookup::new(&state, &place_id);
    if !crate::scene_perms::is_scene_owner_or_admin_for(&state, &place, sf.signer.as_str()).await? {
        return Err(crate::http::forbidden(
            "signer is not an owner or admin of this scene",
        ));
    }
    let ctx = RoomContext::from_metadata(&sf.metadata, &place_id);
    let rooms = room_metadata_sync::resolve_rooms_for(&state, &ctx, &place).await?;
    state.scene_bans.unban(&place_id, &banned_address).await?;
    room_metadata_sync::remove_ban(&state, &rooms, &banned_address).await;
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
