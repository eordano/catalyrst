use std::collections::BTreeSet;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::auth_chain::verify_signed_fetch;
use crate::http::{auth_error, ApiError};
use crate::ports::extra_addresses;
use crate::ports::scene_admin::SceneAdminRow;
use crate::AppState;

const SCENE_SIGNER: &str = "decentraland-kernel-scene";
const SERVER_SIGNER: &str = "dcl:authoritative-server";

fn admin_entry(a: &SceneAdminRow, name: String, can_be_removed: bool) -> serde_json::Value {
    serde_json::json!({
        "id": a.id,
        "place_id": a.place_id,
        "admin": a.admin,
        "added_by": a.added_by,
        "created_at": a.created_at,
        "active": a.active,
        "name": name,
        "canBeRemoved": can_be_removed,
    })
}

fn address_entry(address: &str, name: String) -> serde_json::Value {
    serde_json::json!({
        "admin": address,
        "name": name,
        "canBeRemoved": false,
    })
}

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

    let admins = state
        .scene_admin
        .list_active_admins(&place_id, q.admin.as_deref())
        .await?;

    let (extra_addresses, lease_holders) =
        match extra_addresses::load_place_info(&state, &place_id).await {
            Some(place) => {
                let extra = extra_addresses::get_extra_addresses(&state, &place).await;

                let leases = if place.world {
                    BTreeSet::new()
                } else {
                    extra_addresses::get_lease_holders_for_parcels(&state, &place.positions).await
                };
                (extra, leases)
            }
            None => (BTreeSet::new(), BTreeSet::new()),
        };

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

    for a in &admins {
        let admin_lc = a.admin.to_lowercase();
        let can_be_removed = !extra_addresses.contains(&admin_lc);
        body.push(admin_entry(a, name_of(&admin_lc), can_be_removed));
    }

    for address in &extra_addresses {
        body.push(address_entry(address, name_of(address)));
    }

    for address in &lease_holders {
        body.push(address_entry(address, name_of(address)));
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
    crate::room_metadata_sync::add_admin(&state, &body.place_id, &body.admin).await;
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
    crate::room_metadata_sync::remove_admin(&state, &place_id, &admin).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn row() -> SceneAdminRow {
        SceneAdminRow {
            id: Uuid::nil(),
            place_id: "place-1".into(),
            admin: "0xADMIN".into(),
            added_by: "0xADDER".into(),
            created_at: 1_700_000_000_000,
            active: true,
        }
    }

    #[test]
    fn list_body_is_bare_array() {
        let body = serde_json::Value::Array(vec![
            admin_entry(&row(), "alice.dcl.eth".into(), true),
            address_entry("0xextra", String::new()),
        ]);
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 2);
    }

    #[test]
    fn explicit_admin_entry_matches_upstream_shape() {
        let e = admin_entry(&row(), "alice.dcl.eth".into(), true);
        assert_eq!(e["id"], Uuid::nil().to_string());
        assert_eq!(e["place_id"], "place-1");
        assert_eq!(e["admin"], "0xADMIN");
        assert_eq!(e["added_by"], "0xADDER");
        assert_eq!(e["created_at"], 1_700_000_000_000i64);
        assert_eq!(e["active"], true);
        assert_eq!(e["name"], "alice.dcl.eth");
        assert_eq!(e["canBeRemoved"], true);

        assert!(e.get("can_be_removed").is_none());
    }

    #[test]
    fn extra_admin_entry_is_address_only_and_not_removable() {
        let e = address_entry("0xextra", String::new());
        assert_eq!(e["admin"], "0xextra");
        assert_eq!(e["name"], "");
        assert_eq!(e["canBeRemoved"], false);
        assert!(e.get("id").is_none());
        assert!(e.get("place_id").is_none());
    }
}
