use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use catalyrst_commons::cache::{TtlCell, TtlMap};
use serde::Deserialize;
use sqlx::Row;

use crate::http::{encode_path_segment, ApiError};
use crate::AppState;

const LEASE_AUTHORIZATIONS_URL: &str =
    "https://decentraland.github.io/linker-server-authorizations/authorizations.json";

pub const WORLD_PERMISSIONS_UNAVAILABLE_MSG: &str =
    "the world permission service is unavailable, so this ban target cannot be checked for \
     protection";

pub const LAND_OPERATORS_UNAVAILABLE_MSG: &str =
    "the land permission service is unavailable, so this ban target cannot be checked for \
     protection";

const WORLD_PERMISSIONS_TTL: Duration = Duration::from_secs(300);
const WORLD_PERMISSIONS_MAX_ENTRIES: usize = 5000;
const WORLD_ABOUT_TTL: Duration = Duration::from_secs(60);
const LEASE_AUTHORIZATIONS_TTL: Duration = Duration::from_secs(300);

pub struct PlaceInfo {
    pub world: bool,
    pub world_name: Option<String>,

    pub positions: Vec<String>,
    pub base_position: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LandOperators {
    owner: String,
    #[serde(default)]
    operator: Option<String>,
    #[serde(default, rename = "updateOperator")]
    update_operator: Option<String>,
    #[serde(default, rename = "updateManagers")]
    update_managers: Vec<String>,
    #[serde(default, rename = "approvedForAll")]
    approved_for_all: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ParcelAddressesResponse {
    #[serde(default)]
    addresses: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorldPermissions {
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    permissions: Option<WorldPermissionSettings>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorldPermissionSettings {
    #[serde(default)]
    deployment: Option<AllowListSetting>,
    #[serde(default)]
    access: Option<AllowListSetting>,
    #[serde(default)]
    streaming: Option<AllowListSetting>,
}

#[derive(Debug, Clone, Deserialize)]
struct AllowListSetting {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    wallets: Vec<String>,
}

/// Upstream reads `GET /world/:name/permissions` through a URL-keyed LRU
/// (`cachedFetch.cache<PermissionsOverWorld>()`, 5 min TTL, 5000 entries, misses
/// on error), shared by the world-access gate and the extra-address lookup. Same
/// scope here: keyed on the request URL, so a second join for the same world on
/// the same content server inside the TTL costs no fetch. `TtlMap::bounded`
/// flushes the whole map on overflow rather than evicting the oldest entry, so
/// keep the cap far above the live world count or every join pays a refetch.
#[derive(Clone)]
pub struct WorldPermissionsCache {
    entries: Arc<TtlMap<String, WorldPermissions>>,
    /// World `/about` scene ids keyed on the request URL; a failed or empty
    /// resolution is never stored.
    pub(crate) about_scene_ids: Arc<TtlMap<String, String>>,
    lease_holders: Arc<TtlCell<Arc<Vec<LeaseAuthorization>>>>,
}

impl Default for WorldPermissionsCache {
    fn default() -> Self {
        Self {
            entries: Arc::new(TtlMap::bounded(
                "comms-world-permissions",
                WORLD_PERMISSIONS_TTL,
                WORLD_PERMISSIONS_MAX_ENTRIES,
            )),
            about_scene_ids: Arc::new(TtlMap::bounded(
                "comms-world-about",
                WORLD_ABOUT_TTL,
                WORLD_PERMISSIONS_MAX_ENTRIES,
            )),
            lease_holders: Arc::new(TtlCell::new("comms-lease-authorizations")),
        }
    }
}

impl WorldPermissionsCache {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Deserialize)]
struct LeaseAuthorization {
    #[serde(default)]
    addresses: Vec<String>,
    #[serde(default)]
    plots: Vec<String>,
}

fn parse_xy(s: &str) -> Option<(i32, i32)> {
    let mut it = s.splitn(2, ',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
}

/// Best-effort read: a places-DB fault is indistinguishable from a missing row
/// here, so anything that has to answer differently on a fault calls
/// `try_load_place_info` instead.
pub async fn load_place_info(state: &AppState, place_id: &str) -> Option<PlaceInfo> {
    try_load_place_info(state, place_id).await.ok().flatten()
}

pub async fn try_load_place_info(
    state: &AppState,
    place_id: &str,
) -> Result<Option<PlaceInfo>, sqlx::Error> {
    let Some(pool) = state.places_pool.as_ref() else {
        return Ok(None);
    };
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {PLACE_INFO_FIELDS} FROM place WHERE id = $1"
    )))
    .bind(place_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(place_info_from_row))
}

pub(crate) const PLACE_INFO_FIELDS: &str = "COALESCE((raw->>'world')::bool, false) AS world, \
     raw->>'world_name' AS world_name, \
     raw->'positions' AS positions, \
     base_position";

pub(crate) fn place_info_from_row(row: &sqlx::postgres::PgRow) -> PlaceInfo {
    let world: bool = row.try_get("world").unwrap_or(false);
    let world_name: Option<String> = row.try_get("world_name").ok().flatten();
    let base_position: Option<String> = row.try_get("base_position").ok();

    let mut positions: Vec<String> = Vec::new();
    if let Ok(serde_json::Value::Array(arr)) = row.try_get::<serde_json::Value, _>("positions") {
        for p in arr {
            if let Some(s) = p.as_str() {
                positions.push(s.to_string());
            }
        }
    }
    if positions.is_empty() {
        if let Some(bp) = base_position.as_deref() {
            positions.push(bp.to_string());
        }
    }

    PlaceInfo {
        world,
        world_name,
        positions,
        base_position,
    }
}

/// One place row shared by every step of a request (authz, target protection,
/// room binding); read lazily so a step that never needed it costs nothing.
pub struct PlaceLookup<'a> {
    state: &'a AppState,
    place_id: String,
    cell: tokio::sync::OnceCell<Result<Option<PlaceInfo>, String>>,
}

impl<'a> PlaceLookup<'a> {
    pub fn new(state: &'a AppState, place_id: &str) -> Self {
        Self {
            state,
            place_id: place_id.to_string(),
            cell: tokio::sync::OnceCell::new(),
        }
    }

    pub fn resolved(state: &'a AppState, place_id: &str, place: Option<PlaceInfo>) -> Self {
        Self {
            state,
            place_id: place_id.to_string(),
            cell: tokio::sync::OnceCell::new_with(Some(Ok(place))),
        }
    }

    pub fn place_id(&self) -> &str {
        &self.place_id
    }

    /// `Err` carries the places-DB error text; callers map it to their own status.
    pub async fn get(&self) -> Result<Option<&PlaceInfo>, &str> {
        self.cell
            .get_or_init(|| async {
                try_load_place_info(self.state, &self.place_id)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .as_ref()
            .map(Option::as_ref)
            .map_err(String::as_str)
    }
}

async fn fetch_world_permissions(state: &AppState, world_name: &str) -> Option<WorldPermissions> {
    let url = format!(
        "{}/world/{}/permissions",
        state.world_content_url,
        encode_path_segment(&world_name.to_lowercase())
    );
    state
        .world_permissions
        .entries
        .get_or_fetch(url.clone(), || async {
            let resp = state.http.get(&url).send().await.map_err(|_| ())?;
            if !resp.status().is_success() {
                return Err(());
            }
            resp.json::<WorldPermissions>().await.map_err(|_| ())
        })
        .await
        .ok()
}

fn world_access_allowed(perms: Option<&WorldPermissions>, identity: &str) -> bool {
    let Some(perms) = perms else {
        return false;
    };
    if perms
        .owner
        .as_deref()
        .is_some_and(|owner| owner.eq_ignore_ascii_case(identity))
    {
        return true;
    }
    let Some(access) = perms.permissions.as_ref().and_then(|p| p.access.as_ref()) else {
        return false;
    };
    match access.kind.as_str() {
        "unrestricted" => true,
        "allow-list" => access
            .wallets
            .iter()
            .any(|wallet| wallet.eq_ignore_ascii_case(identity)),
        _ => false,
    }
}

pub async fn has_world_access_permission(
    state: &AppState,
    identity: &str,
    world_name: &str,
) -> bool {
    let perms = fetch_world_permissions(state, world_name).await;
    world_access_allowed(perms.as_ref(), identity)
}

pub(crate) fn world_access_allowed_bytes(body: &[u8], identity: &str) -> bool {
    let permissions = serde_json::from_slice::<WorldPermissions>(body).ok();
    world_access_allowed(permissions.as_ref(), identity)
}

async fn fetch_world_parcel_permission_addresses(
    state: &AppState,
    world_name: &str,
    permission: &str,
    parcels: &[String],
) -> Result<Vec<String>, ()> {
    if parcels.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "{}/world/{}/permissions/{}/parcels",
        state.world_content_url,
        encode_path_segment(&world_name.to_lowercase()),
        encode_path_segment(permission)
    );
    let resp = state
        .http
        .post(&url)
        .json(&serde_json::json!({ "parcels": parcels }))
        .send()
        .await
        .map_err(|_| ())?;
    if !resp.status().is_success() {
        return Err(());
    }
    let body = resp
        .json::<ParcelAddressesResponse>()
        .await
        .map_err(|_| ())?;
    Ok(body.addresses)
}

/// Upstream's `getLandOperators` (adapters/lands/component.ts) reads through
/// `parcelOperatorsCache.fetch`, whose fetchMethod re-throws on a transport
/// error and on any non-2xx, so a lambdas fault reaches the caller as a
/// rejection and never as "this parcel has no operators".
async fn try_fetch_land_operators(
    state: &AppState,
    parcel: &str,
) -> Result<Option<LandOperators>, ApiError> {
    let Some((x, y)) = parse_xy(parcel) else {
        return Ok(None);
    };

    let base = state.lambdas_url.trim_end_matches('/');
    let url = format!("{base}/parcels/{x}/{y}/operators");
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|_| crate::http::service_unavailable(LAND_OPERATORS_UNAVAILABLE_MSG))?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(crate::http::service_unavailable(
            LAND_OPERATORS_UNAVAILABLE_MSG,
        ));
    }
    resp.json::<LandOperators>()
        .await
        .map(Some)
        .map_err(|_| crate::http::service_unavailable(LAND_OPERATORS_UNAVAILABLE_MSG))
}

/// Swallows a permission-surface outage into a short list. Anything deciding
/// whether an address is PROTECTED must use `try_get_extra_addresses` instead:
/// an empty list there reads as "nobody is protected".
pub async fn get_extra_addresses(state: &AppState, place: &PlaceInfo) -> BTreeSet<String> {
    try_get_extra_addresses(state, place)
        .await
        .unwrap_or_default()
}

/// Upstream reads the same surface through `getUserScenePermissions`
/// (adapters/scene-manager.ts) and `getAdminsAndExtraAddresses`
/// (adapters/scene-admins.ts), where a permission fetch that cannot run throws
/// rather than reporting the flags as false, so a ban target is never cleared by
/// an outage. A world whose permission document cannot be read is an outage on
/// both the fast path and the allow-list fallback, and so is a genesis parcel
/// whose land operators cannot be read; a surface that legitimately lists no
/// extra address answers with an empty set.
pub async fn try_get_extra_addresses(
    state: &AppState,
    place: &PlaceInfo,
) -> Result<BTreeSet<String>, ApiError> {
    let mut extra: BTreeSet<String> = BTreeSet::new();

    if place.world {
        let Some(world_name) = place.world_name.as_deref() else {
            return Ok(extra);
        };

        let (deployment, streaming, perms) = tokio::join!(
            fetch_world_parcel_permission_addresses(
                state,
                world_name,
                "deployment",
                &place.positions
            ),
            fetch_world_parcel_permission_addresses(
                state,
                world_name,
                "streaming",
                &place.positions
            ),
            fetch_world_permissions(state, world_name),
        );
        let Some(perms) = perms else {
            return Err(crate::http::service_unavailable(
                WORLD_PERMISSIONS_UNAVAILABLE_MSG,
            ));
        };

        match (deployment, streaming) {
            (Ok(dep), Ok(stream)) => {
                for a in dep {
                    extra.insert(a.to_lowercase());
                }
                for a in stream {
                    extra.insert(a.to_lowercase());
                }
                if let Some(owner) = perms.owner.as_deref() {
                    extra.insert(owner.to_lowercase());
                }
            }
            _ => {
                if let Some(settings) = perms.permissions.as_ref() {
                    if let Some(dep) = settings.deployment.as_ref() {
                        if dep.kind == "allow-list" {
                            for w in &dep.wallets {
                                extra.insert(w.to_lowercase());
                            }
                        }
                    }
                    if let Some(stream) = settings.streaming.as_ref() {
                        if stream.kind == "allow-list" {
                            for w in &stream.wallets {
                                extra.insert(w.to_lowercase());
                            }
                        }
                    }
                }
                if let Some(owner) = perms.owner.as_deref() {
                    extra.insert(owner.to_lowercase());
                }
            }
        }
    } else {
        let parcel = place
            .base_position
            .clone()
            .or_else(|| place.positions.first().cloned());
        if let Some(parcel) = parcel {
            if let Some(ops) = try_fetch_land_operators(state, &parcel).await? {
                extra.insert(ops.owner.to_lowercase());
                if let Some(op) = ops.operator {
                    extra.insert(op.to_lowercase());
                }
                if let Some(op) = ops.update_operator {
                    extra.insert(op.to_lowercase());
                }
                for op in ops.update_managers {
                    extra.insert(op.to_lowercase());
                }
                for op in ops.approved_for_all {
                    extra.insert(op.to_lowercase());
                }
            }
        }
    }

    Ok(extra)
}

pub async fn get_lease_holders_for_parcels(
    state: &AppState,
    parcels: &[String],
) -> BTreeSet<String> {
    let mut holders: BTreeSet<String> = BTreeSet::new();
    if parcels.is_empty() {
        return holders;
    }

    let Some(auths) = state
        .world_permissions
        .lease_holders
        .get_or_refresh_backoff(LEASE_AUTHORIZATIONS_TTL, || async {
            let resp = state
                .http
                .get(LEASE_AUTHORIZATIONS_URL)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("lease authorizations returned {}", resp.status()));
            }
            resp.json::<Vec<LeaseAuthorization>>()
                .await
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
        .await
    else {
        return holders;
    };

    let parcel_set: BTreeSet<&str> = parcels.iter().map(String::as_str).collect();
    for auth in auths.iter() {
        let overlaps = auth
            .plots
            .iter()
            .any(|plot| parcel_set.contains(plot.as_str()));
        if overlaps {
            for addr in &auth.addresses {
                holders.insert(addr.to_lowercase());
            }
        }
    }
    holders
}

#[cfg(test)]
mod tests {
    use super::{parse_xy, world_access_allowed, WorldPermissions};
    use serde_json::json;

    const IDENTITY: &str = "0xAbC0000000000000000000000000000000000001";

    fn perms(value: serde_json::Value) -> WorldPermissions {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn parse_xy_handles_coords() {
        assert_eq!(parse_xy("-100,37"), Some((-100, 37)));
        assert_eq!(parse_xy(" 12 , -5 "), Some((12, -5)));
        assert_eq!(parse_xy("0,0"), Some((0, 0)));
        assert_eq!(parse_xy("bad"), None);
    }

    #[test]
    fn world_owner_is_allowed_case_insensitively() {
        let p = perms(json!({
            "owner": IDENTITY.to_lowercase(),
            "permissions": { "access": { "type": "allow-list", "wallets": [] } }
        }));
        assert!(world_access_allowed(Some(&p), IDENTITY));
        assert!(world_access_allowed(Some(&p), &IDENTITY.to_lowercase()));
    }

    #[test]
    fn unrestricted_access_is_allowed() {
        let p = perms(json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": { "access": { "type": "unrestricted" } }
        }));
        assert!(world_access_allowed(Some(&p), IDENTITY));
    }

    #[test]
    fn allow_list_hit_is_allowed_and_miss_is_denied() {
        let hit = perms(json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": {
                "access": { "type": "allow-list", "wallets": [IDENTITY.to_lowercase()] }
            }
        }));
        assert!(world_access_allowed(Some(&hit), IDENTITY));

        let miss = perms(json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": {
                "access": {
                    "type": "allow-list",
                    "wallets": ["0x1111111111111111111111111111111111111111"]
                }
            }
        }));
        assert!(!world_access_allowed(Some(&miss), IDENTITY));
    }

    #[test]
    fn other_access_types_and_a_failed_lookup_deny() {
        assert!(!world_access_allowed(None, IDENTITY));
        for kind in ["shared-secret", "nft-ownership"] {
            let p = perms(json!({
                "owner": "0x9999999999999999999999999999999999999999",
                "permissions": { "access": { "type": kind, "wallets": [IDENTITY] } }
            }));
            assert!(!world_access_allowed(Some(&p), IDENTITY), "{kind}");
        }
        let no_access = perms(json!({
            "owner": "0x9999999999999999999999999999999999999999",
            "permissions": { "deployment": { "type": "allow-list", "wallets": [IDENTITY] } }
        }));
        assert!(!world_access_allowed(Some(&no_access), IDENTITY));
        assert!(!world_access_allowed(Some(&perms(json!({}))), IDENTITY));
    }
}
