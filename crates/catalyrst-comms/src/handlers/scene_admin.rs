use std::collections::BTreeSet;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, ApiError};
use crate::ports::extra_addresses;
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
    let sf = verify_signed_fetch(
        &headers,
        "get",
        "/scene-admin",
        &[SCENE_SIGNER, SERVER_SIGNER],
    )
    .map_err(|e| auth_error(e.status, e.message))?;
    let place_id = q
        .place_id
        .or_else(|| super::scene_adapter::place_from_metadata(&sf.metadata))
        .ok_or_else(|| ApiError::bad_request("missing place_id query"))?;

    // Explicit scene admins (optionally filtered by the `admin` query, mirroring
    // upstream's `getAdminsAndExtraAddresses(place, admin)` — the filter applies
    // only to the explicit rows, never to the implicit extra addresses).
    let admins = state
        .scene_admin
        .list_active_admins(&place_id, q.admin.as_deref())
        .await?;

    // Implicit (extra) administrators + off-chain land-lease holders. Both
    // degrade to empty when the place is unknown or an upstream source is
    // unavailable, so the explicit admin list is never dropped.
    let (extra_addresses, lease_holders) =
        match extra_addresses::load_place_info(&state, &place_id).await {
            Some(place) => {
                let extra = extra_addresses::get_extra_addresses(&state, &place).await;
                // Land lease only applies to Genesis City scenes; skip for worlds.
                let leases = if place.world {
                    BTreeSet::new()
                } else {
                    extra_addresses::get_lease_holders_for_parcels(&state, &place.positions).await
                };
                (extra, leases)
            }
            None => (BTreeSet::new(), BTreeSet::new()),
        };

    // Combined address set for a single batched name lookup (admins + extra +
    // lease), computed before resolving names — matching upstream.
    let mut all_addresses: BTreeSet<String> = BTreeSet::new();
    for a in &admins {
        all_addresses.insert(a.admin.to_lowercase());
    }
    all_addresses.extend(extra_addresses.iter().cloned());
    all_addresses.extend(lease_holders.iter().cloned());

    let lookup: Vec<String> = all_addresses.iter().cloned().collect();
    let names = state.names.get_names_from_addresses(&lookup).await;
    let name_of = |addr: &str| names.get(addr).cloned().unwrap_or_default();

    let mut body: Vec<serde_json::Value> = Vec::new();

    // Explicit admins: full SceneAdmin row spread + name + canBeRemoved.
    for a in &admins {
        let admin_lc = a.admin.to_lowercase();
        body.push(serde_json::json!({
            "id": a.id,
            "place_id": a.place_id,
            "admin": a.admin,
            "added_by": a.added_by,
            "created_at": a.created_at,
            "active": a.active,
            "name": name_of(&admin_lc),
            // An admin that is also an implicit (extra) grant cannot be revoked.
            "canBeRemoved": !extra_addresses.contains(&admin_lc),
        }));
    }

    // Extra (implicit) admins: address-only entries, never removable.
    for address in &extra_addresses {
        body.push(serde_json::json!({
            "admin": address,
            "name": name_of(address),
            "canBeRemoved": false,
        }));
    }

    // Land-lease owners: address-only entries, never removable.
    for address in &lease_holders {
        body.push(serde_json::json!({
            "admin": address,
            "name": name_of(address),
            "canBeRemoved": false,
        }));
    }

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
    state
        .scene_admin
        .add(&body.place_id, &body.admin, &sf.signer)
        .await?;
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
