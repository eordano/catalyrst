use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::access::AccessSetting;
use crate::auth_chain::{require_verified, AuthChainError, PERMISSIONS_METADATA_KEYS};
use crate::fed::names::LocalWorldName;
use crate::http::ApiError;
use crate::ports::worlds::{AllowListEdit, AllowListEditOutcome, WorldProbe};
use crate::AppState;

const MAX_WALLETS: usize = 1000;
const MAX_COMMUNITIES: usize = 50;
const DCL_ETH_SUFFIX: &str = ".dcl.eth";
/// Squid ENS ownership is memoized for the public GET only; owner-gated writes stay fresh.
const SQUID_OWNER_MEMO_TTL: Duration = Duration::from_secs(300);

fn squid_owner_memo() -> &'static moka::future::Cache<String, Option<String>> {
    static MEMO: OnceLock<moka::future::Cache<String, Option<String>>> = OnceLock::new();
    MEMO.get_or_init(|| {
        moka::future::Cache::builder()
            .time_to_live(SQUID_OWNER_MEMO_TTL)
            .max_capacity(50_000)
            .build()
    })
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct AllowListPermission {
    #[serde(rename = "type")]
    pub kind: String,
    pub wallets: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct WorldPermissionsBlock {
    pub deployment: AllowListPermission,
    pub streaming: AllowListPermission,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub access: Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct PermissionSummaryEntry {
    pub permission: String,
    pub world_wide: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number | null", optional))]
    pub parcel_count: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct PermissionsResponse {
    pub permissions: WorldPermissionsBlock,
    pub owner: Option<String>,
    pub summary: BTreeMap<String, Vec<PermissionSummaryEntry>>,
}

#[utoipa::path(
    get,
    path = "/world/{world_name}/permissions",
    tag = "permissions",
    params(("world_name" = String, Path)),
    responses(
        (status = 200, body = PermissionsResponse),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_permissions(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> Result<Json<PermissionsResponse>, ApiError> {
    let (world, records) = state
        .worlds
        .get_world_with_permission_records(&world_name)
        .await?;
    let access = world.as_ref().map(|w| w.access.clone()).unwrap_or_default();

    let owner = resolve_world_owner_with(
        &state,
        &LocalWorldName::from_request_path(&world_name),
        world.as_ref().and_then(|w| w.owner.clone()),
        true,
    )
    .await;

    let mut deployment_wallets: Vec<String> = Vec::new();
    let mut streaming_wallets: Vec<String> = Vec::new();
    let mut summary: BTreeMap<String, Vec<PermissionSummaryEntry>> = BTreeMap::new();

    for r in &records {
        match r.permission_type.as_str() {
            "deployment" => deployment_wallets.push(r.address.clone()),
            "streaming" => streaming_wallets.push(r.address.clone()),
            _ => {}
        }
        summary
            .entry(r.address.clone())
            .or_default()
            .push(PermissionSummaryEntry {
                permission: r.permission_type.clone(),
                world_wide: r.is_world_wide,
                parcel_count: if r.is_world_wide {
                    None
                } else {
                    Some(r.parcel_count)
                },
            });
    }

    Ok(Json(PermissionsResponse {
        permissions: WorldPermissionsBlock {
            deployment: AllowListPermission {
                kind: "allow-list".to_string(),
                wallets: deployment_wallets,
            },
            streaming: AllowListPermission {
                kind: "allow-list".to_string(),
                wallets: streaming_wallets,
            },
            access: access.to_public_json(),
        },
        owner,
        summary,
    }))
}

/// Owner lookup order: personal-worlds config, the stored column, then squid ENS; takes a
/// [`LocalWorldName`] so a peer-reported [`crate::fed::names::RemoteWorldName`] can never reach it.
pub(crate) async fn resolve_world_owner(
    state: &AppState,
    world_name: &LocalWorldName,
    stored_owner: Option<String>,
) -> Option<String> {
    resolve_world_owner_with(state, world_name, stored_owner, false).await
}

async fn resolve_world_owner_with(
    state: &AppState,
    world_name: &LocalWorldName,
    stored_owner: Option<String>,
    memo: bool,
) -> Option<String> {
    if let Some(owner) = state.cfg.personal_worlds.owner_of(world_name.as_str()) {
        return Some(owner);
    }
    if let Some(owner) = stored_owner {
        return Some(owner);
    }
    let pool = state.squid_pool.as_ref()?;
    let lowered = world_name.as_str();
    let label = lowered.strip_suffix(DCL_ETH_SUFFIX).unwrap_or(lowered);
    if memo {
        if let Some(hit) = squid_owner_memo().get(label).await {
            return hit;
        }
    }
    match resolve_name_owner_id(pool, label).await {
        Ok(found) => {
            let owner = found.and_then(|id| id.split('-').next().map(|a| a.to_lowercase()));
            if memo {
                squid_owner_memo()
                    .insert(label.to_string(), owner.clone())
                    .await;
            }
            owner
        }
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
        "SELECT n.owner_id FROM squid_marketplace.nft n
         JOIN squid_marketplace.ens e ON n.ens_id = e.id
         WHERE n.category = 'ens' AND lower(e.subdomain) = lower($1)",
    )
    .bind(label)
    .fetch_optional(pool)
    .await
}

struct OwnerCheck {
    signer: String,
    /// Whether the `worlds` row already exists, so writers can skip creating it.
    world_found: bool,
    /// Only evaluated when asked for (`probe_scenes`).
    has_scenes: bool,
}

async fn verify_owner(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    method: &str,
    world_name: &str,
    probe_scenes: bool,
) -> Result<OwnerCheck, ApiError> {
    let auth = require_verified(headers, method, path, &[])
        .await
        .map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();

    let lookup = state
        .worlds
        .lookup_world(
            world_name,
            WorldProbe {
                has_scenes: probe_scenes,
                ..WorldProbe::default()
            },
        )
        .await?;
    let world_found = lookup.world.is_some();
    let owner = resolve_world_owner(
        state,
        &LocalWorldName::from_request_path(world_name),
        lookup.world.and_then(|w| w.owner),
    )
    .await;
    let is_owner = owner
        .as_deref()
        .map(|o| o.eq_ignore_ascii_case(&signer))
        .unwrap_or(false);
    if !is_owner {
        return Err(ApiError::forbidden(format!(
            "Your wallet does not own \"{world_name}\", you can not set access control lists for it."
        )));
    }
    Ok(OwnerCheck {
        signer,
        world_found,
        has_scenes: lookup.has_scenes,
    })
}

pub(crate) fn map_auth_error(e: AuthChainError) -> ApiError {
    match e {
        AuthChainError::MissingTimestamp
        | AuthChainError::MalformedChain { .. }
        | AuthChainError::InsufficientLinks => ApiError::bad_request(e.http_message()),
        _ => ApiError::unauthorized(e.to_string()),
    }
}

fn is_allow_list_permission(p: &str) -> bool {
    p == "deployment" || p == "streaming"
}

fn is_permission_with_wallet_support(p: &str) -> bool {
    p == "deployment" || p == "streaming" || p == "access"
}

#[utoipa::path(
    post,
    path = "/world/{world_name}/permissions/{permission_name}",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn post_permissions(
    State(state): State<AppState>,
    Path((world_name, permission_name)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let auth = require_verified(&headers, "post", uri.path(), PERMISSIONS_METADATA_KEYS)
        .await
        .map_err(map_auth_error)?;
    let signer = auth.signer.as_str().to_string();

    let world = state.worlds.get_world(&world_name).await?;
    let owner = resolve_world_owner(
        &state,
        &LocalWorldName::from_request_path(&world_name),
        world.and_then(|w| w.owner),
    )
    .await;
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
        .replace_world_wide_permission(world_name, owner, permission, wallets)
        .await
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
        .store_access_for_owner(world_name, owner, &access)
        .await
}

#[utoipa::path(
    put,
    path = "/world/{world_name}/permissions/{permission_name}/{address}",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path), ("address" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
    let owner = verify_owner(&state, &headers, uri.path(), "put", &world_name, false).await?;
    let ensure_owner = (!owner.world_found).then_some(owner.signer.as_str());

    if is_allow_list_permission(&permission_name) {
        state
            .worlds
            .grant_addresses_world_wide_permission(
                &world_name,
                &permission_name,
                &[address.to_lowercase()],
                ensure_owner,
            )
            .await?;
    } else {
        if let Some(signer) = ensure_owner {
            state
                .worlds
                .create_basic_world_if_not_exists(&world_name, signer)
                .await?;
        }
        add_wallet_to_access(&state, &world_name, &address).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/world/{world_name}/permissions/{permission_name}/{address}",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path), ("address" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
    let owner = verify_owner(&state, &headers, uri.path(), "delete", &world_name, true).await?;
    if !owner.has_scenes {
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

#[utoipa::path(
    post,
    path = "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path), ("address" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn post_permission_parcels(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(input): Json<ParcelsInput>,
) -> Result<StatusCode, ApiError> {
    validate_address_and_allow_list(&address, &permission_name)?;
    verify_owner(&state, &headers, uri.path(), "post", &world_name, false).await?;
    state
        .worlds
        .add_parcels_to_permission(&world_name, &permission_name, &address, &input.parcels)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path), ("address" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_permission_parcels(
    State(state): State<AppState>,
    Path((world_name, permission_name, address)): Path<(String, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(input): Json<ParcelsInput>,
) -> Result<StatusCode, ApiError> {
    validate_address_and_allow_list(&address, &permission_name)?;
    verify_owner(&state, &headers, uri.path(), "delete", &world_name, false).await?;

    let found = state
        .worlds
        .remove_parcels_from_permission_by_address(
            &world_name,
            &permission_name,
            &address,
            &input.parcels,
        )
        .await?;
    if !found {
        return Err(ApiError::bad_request(format!(
            "Permission not found. Address {address} does not have {permission_name} permission for world {world_name}."
        )));
    }
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

#[utoipa::path(
    get,
    path = "/world/{world_name}/permissions/{permission_name}/address/{address}/parcels",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path), ("address" = String, Path), ("limit" = Option<i64>, Query), ("offset" = Option<i64>, Query)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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

    let (total, parcels) = state
        .worlds
        .get_parcels_for_permission_by_address(
            &world_name,
            &permission_name,
            &address,
            limit,
            offset,
            bbox,
        )
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "Permission '{permission_name}' not found for address {address} in world {world_name}."
            ))
        })?;
    Ok(Json(json!({ "total": total, "parcels": parcels })))
}

#[utoipa::path(
    post,
    path = "/world/{world_name}/permissions/{permission_name}/parcels",
    tag = "permissions",
    params(("world_name" = String, Path), ("permission_name" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
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
    (
        catalyrst_types::clamp_limit(limit, 100, 1000),
        offset.unwrap_or(0).max(0),
    )
}

#[utoipa::path(
    put,
    path = "/world/{world_name}/permissions/access/communities/{communityId}",
    tag = "permissions",
    params(("world_name" = String, Path), ("communityId" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn put_permissions_access_community(
    State(state): State<AppState>,
    Path((world_name, community_id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("Invalid community id."));
    }
    verify_owner(&state, &headers, uri.path(), "put", &world_name, false).await?;
    add_community_to_access(&state, &world_name, &community_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/world/{world_name}/permissions/access/communities/{communityId}",
    tag = "permissions",
    params(("world_name" = String, Path), ("communityId" = String, Path)),
    responses(
        (status = 204),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 401, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn delete_permissions_access_community(
    State(state): State<AppState>,
    Path((world_name, community_id)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if community_id.trim().is_empty() {
        return Err(ApiError::bad_request("Invalid community id."));
    }
    verify_owner(&state, &headers, uri.path(), "delete", &world_name, false).await?;
    remove_community_from_access(&state, &world_name, &community_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn not_allow_list(world_name: &str) -> ApiError {
    ApiError::bad_request(format!(
        "World \"{world_name}\" does not have allow-list access type."
    ))
}

async fn apply_allow_list_edit(
    state: &AppState,
    world_name: &str,
    edit: AllowListEdit<'_>,
    cap_error: impl FnOnce() -> ApiError,
) -> Result<(), ApiError> {
    match state
        .worlds
        .modify_allow_list_access(world_name, edit)
        .await?
    {
        AllowListEditOutcome::Applied => Ok(()),
        AllowListEditOutcome::NotAllowList => Err(not_allow_list(world_name)),
        AllowListEditOutcome::CapExceeded => Err(cap_error()),
    }
}

async fn add_wallet_to_access(
    state: &AppState,
    world_name: &str,
    wallet: &str,
) -> Result<(), ApiError> {
    let lower = wallet.to_lowercase();
    apply_allow_list_edit(state, world_name, AllowListEdit::AddWallet(&lower), || {
        ApiError::bad_request(format!(
            "Cannot add wallet: allow-list would exceed the maximum of {MAX_WALLETS} wallets."
        ))
    })
    .await
}

async fn remove_wallet_from_access(
    state: &AppState,
    world_name: &str,
    wallet: &str,
) -> Result<(), ApiError> {
    let lower = wallet.to_lowercase();
    apply_allow_list_edit(
        state,
        world_name,
        AllowListEdit::RemoveWallet(&lower),
        || ApiError::internal("unreachable: removals have no cap"),
    )
    .await
}

async fn add_community_to_access(
    state: &AppState,
    world_name: &str,
    community_id: &str,
) -> Result<(), ApiError> {
    apply_allow_list_edit(
        state,
        world_name,
        AllowListEdit::AddCommunity(community_id),
        || {
            ApiError::bad_request(format!(
                "Too many communities. Maximum allowed is {MAX_COMMUNITIES}, cannot add more."
            ))
        },
    )
    .await
}

async fn remove_community_from_access(
    state: &AppState,
    world_name: &str,
    community_id: &str,
) -> Result<(), ApiError> {
    apply_allow_list_edit(
        state,
        world_name,
        AllowListEdit::RemoveCommunity(community_id),
        || ApiError::internal("unreachable: removals have no cap"),
    )
    .await
}
