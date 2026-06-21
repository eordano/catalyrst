//! Scene-admin extra-address resolution — port of comms-gatekeeper
//! `sceneAdmins.getAdminsAndExtraAddresses` (`adapters/scene-admins.ts`) and
//! `lands.getLeaseHoldersForParcels` (`adapters/lands/component.ts`).
//!
//! The GET /scene-admin response is not just the explicit `scene_admin` rows: it
//! also surfaces every address that *implicitly* administers the place (world
//! deployment/streaming ACL wallets + world owner for worlds; LAND owner /
//! operator / updateOperator / updateManagers / approvedForAll for Genesis City),
//! plus the off-chain land-lease holders. Those extra addresses are emitted with
//! `canBeRemoved: false`, and any explicit admin that is *also* an extra address
//! becomes `canBeRemoved: false` too (you can't revoke an implicit grant).
//!
//! Each upstream source degrades to empty on failure (a flaky lambdas / world
//! content server must not drop the whole admin list), matching the upstream
//! try/catch + swallow behaviour.

use std::collections::BTreeSet;

use serde::Deserialize;
use sqlx::Row;

use crate::AppState;

const LEASE_AUTHORIZATIONS_URL: &str =
    "https://decentraland.github.io/linker-server-authorizations/authorizations.json";

/// Place fields needed to resolve the extra-address set.
pub struct PlaceInfo {
    pub world: bool,
    pub world_name: Option<String>,
    /// Parcel coordinates ("x,y") of the place.
    pub positions: Vec<String>,
    pub base_position: Option<String>,
}

/// LAND on-chain operators, mirroring upstream `LandsParcelOperatorsResponse`
/// (`adapters/lands/types.ts`): the lambdas `parcels/:x/:y/operators` payload.
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

// --- World action permissions (subset used here) ---

#[derive(Debug, Deserialize)]
struct WorldPermissions {
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    permissions: Option<WorldPermissionSettings>,
}

#[derive(Debug, Deserialize)]
struct WorldPermissionSettings {
    #[serde(default)]
    deployment: Option<AllowListSetting>,
    #[serde(default)]
    streaming: Option<AllowListSetting>,
}

/// An allow-list permission setting (`{ type, wallets }`). `wallets` is absent
/// for non-allow-list types (`unrestricted`/`shared-secret`/`nft-ownership`), so
/// it defaults to empty — matching upstream's `type === AllowList` guard.
#[derive(Debug, Deserialize)]
struct AllowListSetting {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    wallets: Vec<String>,
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

/// Load the place's world flag / world-name / positions / base position from the
/// places_events archive. Returns `None` when the place is unknown or the pool is
/// unavailable (callers then emit only the explicit admins).
pub async fn load_place_info(state: &AppState, place_id: &str) -> Option<PlaceInfo> {
    let pool = state.places_pool.as_ref()?;
    let row = sqlx::query(
        "SELECT COALESCE((raw->>'world')::bool, false) AS world, \
                raw->>'world_name' AS world_name, \
                raw->'positions' AS positions, \
                base_position \
         FROM place WHERE id = $1",
    )
    .bind(place_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

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

    Some(PlaceInfo {
        world,
        world_name,
        positions,
        base_position,
    })
}

async fn fetch_world_permissions(state: &AppState, world_name: &str) -> Option<WorldPermissions> {
    let url = format!(
        "{}/world/{}/permissions",
        state.world_content_url,
        world_name.to_lowercase()
    );
    let resp = state.http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<WorldPermissions>().await.ok()
}

/// POST `/world/:world/permissions/:permission/parcels` with `{ parcels }`;
/// ports `getWorldParcelPermissionAddresses`. Returns `Err` on any non-2xx so the
/// caller can fall back to the all-allow-listed-wallets path, exactly as upstream
/// does via its try/catch.
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
        world_name.to_lowercase(),
        permission
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

async fn fetch_land_operators(state: &AppState, parcel: &str) -> Option<LandOperators> {
    let (x, y) = parse_xy(parcel)?;
    // upstream ensureSlashAtTheEnd(lambdasUrl) + `parcels/:x/:y/operators`.
    let base = state.lambdas_url.trim_end_matches('/');
    let url = format!("{base}/parcels/{x}/{y}/operators");
    let resp = state.http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<LandOperators>().await.ok()
}

/// The extra (implicit) administrator addresses for a place. World scenes draw on
/// the world ACL + owner; Genesis City scenes draw on the LAND operator set. All
/// returned addresses are lowercased.
pub async fn get_extra_addresses(state: &AppState, place: &PlaceInfo) -> BTreeSet<String> {
    let mut extra: BTreeSet<String> = BTreeSet::new();

    if place.world {
        let Some(world_name) = place.world_name.as_deref() else {
            return extra;
        };

        // Preferred path: bulk per-parcel deployment/streaming address lookup +
        // owner. If either bulk call fails, fall back to the full allow-list.
        let deployment = fetch_world_parcel_permission_addresses(
            state,
            world_name,
            "deployment",
            &place.positions,
        )
        .await;
        let streaming = fetch_world_parcel_permission_addresses(
            state,
            world_name,
            "streaming",
            &place.positions,
        )
        .await;
        let perms = fetch_world_permissions(state, world_name).await;

        match (deployment, streaming) {
            (Ok(dep), Ok(stream)) => {
                for a in dep {
                    extra.insert(a.to_lowercase());
                }
                for a in stream {
                    extra.insert(a.to_lowercase());
                }
                if let Some(owner) = perms.as_ref().and_then(|p| p.owner.as_deref()) {
                    extra.insert(owner.to_lowercase());
                }
            }
            _ => {
                // Bulk endpoint not available — fall back to all allow-listed wallets.
                if let Some(p) = perms {
                    if let Some(settings) = p.permissions.as_ref() {
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
                    if let Some(owner) = p.owner.as_deref() {
                        extra.insert(owner.to_lowercase());
                    }
                }
            }
        }
    } else {
        // Genesis City: resolve LAND operators for the base parcel.
        let parcel = place
            .base_position
            .clone()
            .or_else(|| place.positions.first().cloned());
        if let Some(parcel) = parcel {
            if let Some(ops) = fetch_land_operators(state, &parcel).await {
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

    extra
}

/// Off-chain land-lease holders for any of `parcels`. Ports
/// `lands.getLeaseHoldersForParcels`: pulls the static linker-server
/// authorizations JSON, then unions the addresses of every authorization whose
/// `plots` overlap `parcels`. Failures degrade to empty (upstream swallows them).
pub async fn get_lease_holders_for_parcels(
    state: &AppState,
    parcels: &[String],
) -> BTreeSet<String> {
    let mut holders: BTreeSet<String> = BTreeSet::new();
    if parcels.is_empty() {
        return holders;
    }

    let resp = match state.http.get(LEASE_AUTHORIZATIONS_URL).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return holders,
    };
    let auths = match resp.json::<Vec<LeaseAuthorization>>().await {
        Ok(a) => a,
        Err(_) => return holders,
    };

    let parcel_set: BTreeSet<&str> = parcels.iter().map(String::as_str).collect();
    for auth in auths {
        let overlaps = auth
            .plots
            .iter()
            .any(|plot| parcel_set.contains(plot.as_str()));
        if overlaps {
            for addr in auth.addresses {
                holders.insert(addr.to_lowercase());
            }
        }
    }
    holders
}

#[cfg(test)]
mod tests {
    use super::parse_xy;

    #[test]
    fn parse_xy_handles_coords() {
        assert_eq!(parse_xy("-100,37"), Some((-100, 37)));
        assert_eq!(parse_xy(" 12 , -5 "), Some((12, -5)));
        assert_eq!(parse_xy("0,0"), Some((0, 0)));
        assert_eq!(parse_xy("bad"), None);
    }
}
