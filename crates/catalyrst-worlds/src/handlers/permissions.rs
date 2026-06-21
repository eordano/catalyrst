use std::collections::BTreeMap;

use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::access::AccessSetting;
use crate::auth_chain::{require_verified, AuthChainError};
use crate::http::ApiError;
use crate::AppState;

const MAX_WALLETS: usize = 1000;
const MAX_COMMUNITIES: usize = 50;
const DCL_ETH_SUFFIX: &str = ".dcl.eth";

pub async fn get_permissions(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let world = state.worlds.get_world(&world_name).await?;
    let access = world.as_ref().map(|w| w.access.clone()).unwrap_or_default();

    let owner = resolve_world_owner(
        &state,
        &world_name,
        world.as_ref().and_then(|w| w.owner.clone()),
    )
    .await;

    let records = state
        .worlds
        .get_world_permission_records_full(&world_name)
        .await?;

    let mut deployment_wallets: Vec<String> = Vec::new();
    let mut streaming_wallets: Vec<String> = Vec::new();
    let mut summary: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for r in &records {
        match r.permission_type.as_str() {
            "deployment" => deployment_wallets.push(r.address.clone()),
            "streaming" => streaming_wallets.push(r.address.clone()),
            _ => {}
        }
        let mut entry = serde_json::Map::new();
        entry.insert("permission".into(), json!(r.permission_type));
        entry.insert("world_wide".into(), json!(r.is_world_wide));
        if !r.is_world_wide {
            entry.insert("parcel_count".into(), json!(r.parcel_count));
        }
        summary
            .entry(r.address.clone())
            .or_default()
            .push(Value::Object(entry));
    }

    let body = json!({
        "permissions": {
            "deployment": { "type": "allow-list", "wallets": deployment_wallets },
            "streaming": { "type": "allow-list", "wallets": streaming_wallets },
            "access": access.to_public_json(),
        },
        "owner": owner,
        "summary": summary,
    });

    Ok(Json(body))
}

pub(crate) async fn resolve_world_owner(
    state: &AppState,
    world_name: &str,
    stored_owner: Option<String>,
) -> Option<String> {
    if let Some(owner) = stored_owner {
        return Some(owner);
    }
    let pool = state.squid_pool.as_ref()?;
    let label = world_name
        .to_lowercase()
        .strip_suffix(DCL_ETH_SUFFIX)
        .map(|s| s.to_string())
        .unwrap_or_else(|| world_name.to_lowercase());
    match resolve_name_owner_id(pool, &label).await {
        Ok(Some(owner_id)) => owner_id.split('-').next().map(|a| a.to_lowercase()),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, world = %world_name, "failed to resolve owner via squid nameOwnership");
            None
        }
    }
}

async fn resolve_name_owner_id(
    pool: &sqlx::PgPool,
    label: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT owner_id FROM squid_marketplace.ens WHERE lower(subdomain)=lower($1)",
    )
    .bind(label)
    .fetch_optional(pool)
    .await
}

async fn verify_owner(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    method: &str,
    world_name: &str,
) -> Result<String, ApiError> {
    let auth = require_verified(headers, method, path).map_err(map_auth_error)?;
    let signer = auth.signer.to_lowercase();

    let world = state.worlds.get_world(world_name).await?;
    let owner = resolve_world_owner(state, world_name, world.and_then(|w| w.owner)).await;
    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);
    if !is_owner {
        return Err(ApiError::forbidden(format!(
            "Your wallet does not own \"{world_name}\", you can not set access control lists for it."
        )));
    }
    Ok(signer)
}

pub(crate) fn map_auth_error(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => ApiError::bad_request(e.to_string()),
        _ => ApiError::unauthorized(e.to_string()),
    }
}

fn is_allow_list_permission(p: &str) -> bool {
    p == "deployment" || p == "streaming"
}

fn is_permission_with_wallet_support(p: &str) -> bool {
    p == "deployment" || p == "streaming" || p == "access"
}

pub async fn post_permissions(
    State(state): State<AppState>,
    Path((world_name, permission_name)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let auth = require_verified(&headers, "post", uri.path()).map_err(map_auth_error)?;
    let signer = auth.signer.to_lowercase();

    let world = state.worlds.get_world(&world_name).await?;
    let owner = resolve_world_owner(&state, &world_name, world.and_then(|w| w.owner)).await;
    if !owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false)
    {
        return Err(ApiError::forbidden(format!(
            "Your wallet does not own \"{world_name}\", you can not set access control lists for it."
        )));
    }

    let meta = &auth.metadata;
    match permission_name.as_str() {
        "deployment" => {
            let ty = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty != "allow-list" {
                return Err(ApiError::bad_request(
                    "Invalid payload received. Deployment permission needs to be 'allow-list'.",
                ));
            }
            let wallets = metadata_wallets(meta);
            set_allow_list_permission(&state, &world_name, &signer, "deployment", &wallets).await?;
        }
        "streaming" => {
            let ty = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ty != "unrestricted" && ty != "allow-list" {
                return Err(ApiError::bad_request(
                    "Invalid payload received. Streaming permission needs to be either 'unrestricted' or 'allow-list'.",
                ));
            }
            let wallets = if ty == "unrestricted" {
                Vec::new()
            } else {
                metadata_wallets(meta)
            };
            set_allow_list_permission(&state, &world_name, &signer, "streaming", &wallets).await?;
        }
        "access" => {
            set_access_from_metadata(&state, &world_name, &signer, meta).await?;
        }
        other => {
            return Err(ApiError::bad_request(format!(
                "Invalid permission name: {other}."
            )));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

fn metadata_wallets(meta: &Value) -> Vec<String> {
    meta.get("wallets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| w.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

async fn set_allow_list_permission(
    state: &AppState,
    world_name: &str,
    owner: &str,
    permission: &str,
    wallets: &[String],
) -> Result<(), ApiError> {
    state
        .worlds
        .create_basic_world_if_not_exists(world_name, owner)
        .await?;

    let records = state
        .worlds
        .get_world_permission_records_full(world_name)
        .await?;
    let current: Vec<String> = records
        .iter()
        .filter(|r| r.permission_type == permission)
        .map(|r| r.address.to_lowercase())
        .collect();
    let new: Vec<String> = wallets.iter().map(|w| w.to_lowercase()).collect();

    let to_remove: Vec<String> = current
        .iter()
        .filter(|a| !new.contains(a))
        .cloned()
        .collect();
    if !to_remove.is_empty() {
        state
            .worlds
            .remove_addresses_permission(world_name, permission, &to_remove)
            .await?;
    }
    let to_add: Vec<String> = new
        .iter()
        .filter(|a| !current.contains(a))
        .cloned()
        .collect();
    if !to_add.is_empty() {
        state
            .worlds
            .grant_addresses_world_wide_permission(world_name, permission, &to_add)
            .await?;
    }
    Ok(())
}

async fn set_access_from_metadata(
    state: &AppState,
    world_name: &str,
    owner: &str,
    meta: &Value,
) -> Result<(), ApiError> {
    let ty = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let access = match ty {
        "unrestricted" => AccessSetting::Unrestricted,
        "nft-ownership" => {
            let nft = meta.get("nft").and_then(|v| v.as_str()).ok_or_else(|| {
                ApiError::bad_request("For nft ownership there needs to be a valid nft.")
            })?;
            AccessSetting::NftOwnership {
                nft: nft.to_string(),
            }
        }
        "shared-secret" => {
            let secret = meta
                .get("secret")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ApiError::bad_request("For shared secret there needs to be a valid secret.")
                })?;
            let hash = bcrypt::hash(secret, bcrypt::DEFAULT_COST)
                .map_err(|e| ApiError::internal(format!("hash secret: {e}")))?;
            AccessSetting::SharedSecret { secret: hash }
        }
        "allow-list" => {
            let wallets: Vec<String> = meta
                .get("wallets")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|w| w.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let communities: Vec<String> = meta
                .get("communities")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if wallets.len() > MAX_WALLETS {
                return Err(ApiError::bad_request(format!(
                    "Too many wallets in allow-list. Maximum allowed is {MAX_WALLETS}, but {} were provided.",
                    wallets.len()
                )));
            }
            if communities.len() > MAX_COMMUNITIES {
                return Err(ApiError::bad_request(format!(
                    "Too many communities. Maximum allowed is {MAX_COMMUNITIES}, but {} were provided.",
                    communities.len()
                )));
            }
            AccessSetting::AllowList {
                wallets,
                communities,
            }
        }
        other => {
            return Err(ApiError::bad_request(format!(
                "Invalid access type: {other}."
            )));
        }
    };

    state
        .worlds
        .create_basic_world_if_not_exists(world_name, owner)
        .await?;
    state.worlds.store_access(world_name, &access).await?;
    Ok(())
}

pub async fn put_permissions_address(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if !catalyrst_types::is_eth_address(&address) {
        return Err(ApiError::bad_request(format!(
            "Invalid address: {address}."
        )));
    }
    if !is_permission_with_wallet_support(&permission_name) {
        return Err(ApiError::bad_request(format!(
            "Invalid permission name: {permission_name}."
        )));
    }
    let signer = verify_owner(&state, &headers, uri.path(), "put", &world_name).await?;

    if is_allow_list_permission(&permission_name) {
        state
            .worlds
            .create_basic_world_if_not_exists(&world_name, &signer)
            .await?;
        state
            .worlds
            .grant_addresses_world_wide_permission(
                &world_name,
                &permission_name,
                &[address.to_lowercase()],
            )
            .await?;
    } else {
        state
            .worlds
            .create_basic_world_if_not_exists(&world_name, &signer)
            .await?;
        add_wallet_to_access(&state, &world_name, &address).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_permissions_address(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if !catalyrst_types::is_eth_address(&address) {
        return Err(ApiError::bad_request(format!(
            "Invalid address: {address}."
        )));
    }
    if !is_permission_with_wallet_support(&permission_name) {
        return Err(ApiError::bad_request(format!(
            "Permission '{permission_name}' does not support allow-list. Only 'deployment', 'streaming', and 'access' do."
        )));
    }
    verify_owner(&state, &headers, uri.path(), "delete", &world_name).await?;

    if !state.worlds.is_world_valid(&world_name).await? {
        return Err(ApiError::not_found(format!(
            "World \"{world_name}\" not found."
        )));
    }

    if is_allow_list_permission(&permission_name) {
        state
            .worlds
            .remove_addresses_permission(&world_name, &permission_name, &[address.to_lowercase()])
            .await?;
    } else {
        remove_wallet_from_access(&state, &world_name, &address).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ParcelsInput {
    #[serde(default)]
    pub parcels: Vec<String>,
}

pub async fn post_permission_parcels(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(input): Json<ParcelsInput>,
) -> Result<StatusCode, ApiError> {
    validate_address_and_allow_list(&address, &permission_name)?;
    verify_owner(&state, &headers, uri.path(), "post", &world_name).await?;
    state
        .worlds
        .add_parcels_to_permission(&world_name, &permission_name, &address, &input.parcels)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_permission_parcels(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(input): Json<ParcelsInput>,
) -> Result<StatusCode, ApiError> {
    validate_address_and_allow_list(&address, &permission_name)?;
    verify_owner(&state, &headers, uri.path(), "delete", &world_name).await?;

    let existing = state
        .worlds
        .get_address_permission_id(&world_name, &permission_name, &address)
        .await?
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "Permission not found. Address {address} does not have {permission_name} permission for world {world_name}."
            ))
        })?;
    state
        .worlds
        .remove_parcels_from_permission(existing, &input.parcels)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ParcelsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub x1: Option<i32>,
    #[serde(default)]
    pub y1: Option<i32>,
    #[serde(default)]
    pub x2: Option<i32>,
    #[serde(default)]
    pub y2: Option<i32>,
}

pub async fn get_allowed_parcels_for_permission(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    Query(q): Query<ParcelsQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_address_and_allow_list(&address, &permission_name)?;

    let bbox = match (q.x1, q.y1, q.x2, q.y2) {
        (Some(x1), Some(y1), Some(x2), Some(y2)) => Some((x1, y1, x2, y2)),
        (None, None, None, None) => None,
        _ => {
            return Err(ApiError::bad_request(
                "Bounding box requires all four parameters: x1, y1, x2, y2.",
            ))
        }
    };
    let (limit, offset) = clamp_pagination(q.limit, q.offset);

    let permission_id = state
        .worlds
        .get_address_permission_id(&world_name, &permission_name, &address)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "Permission '{permission_name}' not found for address {address} in world {world_name}."
            ))
        })?;

    let (total, parcels) = state
        .worlds
        .get_parcels_for_permission(permission_id, limit, offset, bbox)
        .await?;
    Ok(Json(json!({ "total": total, "parcels": parcels })))
}

pub async fn get_addresses_for_parcel_permission(
    State(state): State<AppState>,
    Path((world_name, permission_name)): Path<(String, String)>,
    Query(q): Query<ParcelsQuery>,
    Json(input): Json<ParcelsInput>,
) -> Result<Json<Value>, ApiError> {
    if !is_allow_list_permission(&permission_name) {
        return Err(ApiError::bad_request(format!(
            "Permission '{permission_name}' does not support allow-list. Only 'deployment' and 'streaming' do."
        )));
    }
    let (limit, offset) = clamp_pagination(q.limit, q.offset);
    let (total, addresses) = state
        .worlds
        .get_addresses_for_parcel_permission(
            &world_name,
            &permission_name,
            &input.parcels,
            limit,
            offset,
        )
        .await?;
    Ok(Json(json!({ "total": total, "addresses": addresses })))
}

fn validate_address_and_allow_list(address: &str, permission_name: &str) -> Result<(), ApiError> {
    if !catalyrst_types::is_eth_address(address) {
        return Err(ApiError::bad_request(format!(
            "Invalid address: {address}."
        )));
    }
    if !is_allow_list_permission(permission_name) {
        return Err(ApiError::bad_request(format!(
            "Permission '{permission_name}' does not support allow-list. Only 'deployment' and 'streaming' do."
        )));
    }
    Ok(())
}

fn clamp_pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

pub async fn put_permissions_access_community(
    State(state): State<AppState>,
    Path((world_name, community_id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("Invalid community id."));
    }
    verify_owner(&state, &headers, uri.path(), "put", &world_name).await?;
    add_community_to_access(&state, &world_name, &community_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_permissions_access_community(
    State(state): State<AppState>,
    Path((world_name, community_id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("Invalid community id."));
    }
    verify_owner(&state, &headers, uri.path(), "delete", &world_name).await?;
    remove_community_from_access(&state, &world_name, &community_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn not_allow_list(world_name: &str) -> ApiError {
    ApiError::bad_request(format!(
        "World \"{world_name}\" does not have allow-list access type."
    ))
}

async fn add_wallet_to_access(
    state: &AppState,
    world_name: &str,
    wallet: &str,
) -> Result<(), ApiError> {
    let world = world_name.to_string();
    let lower = wallet.to_lowercase();
    state
        .worlds
        .modify_access_atomically(world_name, move |access| match access {
            AccessSetting::AllowList { mut wallets, communities } => {
                if wallets.iter().any(|w| w.eq_ignore_ascii_case(&lower)) {
                    return Ok(AccessSetting::AllowList { wallets, communities });
                }
                wallets.push(lower);
                if wallets.len() > MAX_WALLETS {
                    return Err(ApiError::bad_request(format!(
                        "Cannot add wallet: allow-list would exceed the maximum of {MAX_WALLETS} wallets."
                    )));
                }
                Ok(AccessSetting::AllowList { wallets, communities })
            }
            _ => Err(not_allow_list(&world)),
        })
        .await?;
    Ok(())
}

async fn remove_wallet_from_access(
    state: &AppState,
    world_name: &str,
    wallet: &str,
) -> Result<(), ApiError> {
    let world = world_name.to_string();
    let lower = wallet.to_lowercase();
    state
        .worlds
        .modify_access_atomically(world_name, move |access| match access {
            AccessSetting::AllowList {
                wallets,
                communities,
            } => Ok(AccessSetting::AllowList {
                wallets: wallets
                    .into_iter()
                    .filter(|w| !w.eq_ignore_ascii_case(&lower))
                    .collect(),
                communities,
            }),
            _ => Err(not_allow_list(&world)),
        })
        .await?;
    Ok(())
}

async fn add_community_to_access(
    state: &AppState,
    world_name: &str,
    community_id: &str,
) -> Result<(), ApiError> {
    let world = world_name.to_string();
    let cid = community_id.to_string();
    state
        .worlds
        .modify_access_atomically(world_name, move |access| match access {
            AccessSetting::AllowList { wallets, mut communities } => {
                if communities.iter().any(|c| c == &cid) {
                    return Ok(AccessSetting::AllowList { wallets, communities });
                }
                if communities.len() >= MAX_COMMUNITIES {
                    return Err(ApiError::bad_request(format!(
                        "Too many communities. Maximum allowed is {MAX_COMMUNITIES}, cannot add more."
                    )));
                }
                communities.push(cid);
                Ok(AccessSetting::AllowList { wallets, communities })
            }
            _ => Err(not_allow_list(&world)),
        })
        .await?;
    Ok(())
}

async fn remove_community_from_access(
    state: &AppState,
    world_name: &str,
    community_id: &str,
) -> Result<(), ApiError> {
    let world = world_name.to_string();
    let cid = community_id.to_string();
    state
        .worlds
        .modify_access_atomically(world_name, move |access| match access {
            AccessSetting::AllowList {
                wallets,
                communities,
            } => Ok(AccessSetting::AllowList {
                wallets,
                communities: communities.into_iter().filter(|c| c != &cid).collect(),
            }),
            _ => Err(not_allow_list(&world)),
        })
        .await?;
    Ok(())
}
