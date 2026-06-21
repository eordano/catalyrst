//! Scene authorization — port of upstream comms-gatekeeper
//! `sceneManager.isSceneOwnerOrAdmin`.
//!
//! The scene-bans and scene-admin write handlers must only let the scene's
//! owner or an explicit scene admin act. `verify_signed_fetch` proves the
//! caller controls *some* key with `metadata.signer = "decentraland-kernel-scene"`,
//! but NOT that they own/administer the target place — so the per-place check
//! here is what actually authorizes the mutation.
//!
//! Resolution: the `place_id` -> parcels/world via the places_events archive,
//! then ownership via the marketplace squid (parcel/estate `owner_address`, or
//! the world-name ENS owner). Upstream additionally honours LAND operators,
//! world streaming/deploy ACLs and land leases; those are not yet ported, so a
//! delegated operator must be added as a scene admin. The check fails CLOSED:
//! if ownership cannot be resolved (pools unavailable / place not found), a
//! non-admin caller is rejected.

use sqlx::Row;

use crate::http::ApiError;
use crate::AppState;

/// The marketplace squid lives in this fixed schema (matches catalyrst-market's
/// `MARKETPLACE_SQUID_SCHEMA`). The `DAPPS_PG_COMPONENT_PSQL_SCHEMA` env var is
/// "marketplace" here, which is a *different* schema and does not hold the
/// parcel/estate/ens `nft` tables — so we pin the real one rather than trust it.
const SQUID_SCHEMA: &str = "squid_marketplace";

fn parse_xy(s: &str) -> Option<(i32, i32)> {
    let mut it = s.splitn(2, ',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
}

/// True iff `signer` may moderate (ban/admin) `place_id`: an explicit scene
/// admin, or the owner of the underlying LAND parcel(s) / world name.
pub async fn is_scene_owner_or_admin(
    state: &AppState,
    place_id: &str,
    signer: &str,
) -> Result<bool, ApiError> {
    let signer = signer.to_lowercase();

    // 1. Explicit scene admin (comms_gatekeeper.scene_admin) — cheap, short-circuit.
    if state.scene_admin.is_admin(place_id, &signer).await? {
        return Ok(true);
    }

    // 2. Owner — resolve the place's parcels/world, then check squid ownership.
    let (Some(places), Some(squid)) = (state.places_pool.as_ref(), state.dapps_pool.as_ref())
    else {
        // Cannot resolve ownership; admin already failed -> deny (fail closed).
        tracing::warn!(
            place_id,
            "scene authz: places/squid pool unavailable; denying non-admin caller"
        );
        return Ok(false);
    };

    let row = sqlx::query(
        "SELECT COALESCE((raw->>'world')::bool, false) AS world, \
                raw->>'world_name' AS world_name, \
                raw->'positions' AS positions, \
                base_position \
         FROM place WHERE id = $1",
    )
    .bind(place_id)
    .fetch_optional(places)
    .await?;

    let Some(row) = row else {
        return Ok(false); // unknown place -> deny
    };

    let schema = SQUID_SCHEMA;
    let is_world: bool = row.try_get("world").unwrap_or(false);

    if is_world {
        // World owner == owner of the world-name ENS.
        let Some(world_name) = row
            .try_get::<Option<String>, _>("world_name")
            .ok()
            .flatten()
        else {
            return Ok(false);
        };
        let base = world_name
            .strip_suffix(".dcl.eth")
            .unwrap_or(&world_name)
            .to_lowercase();
        let q = format!(
            "SELECT 1 FROM {schema}.nft \
             WHERE category = 'ens' AND lower(name) = $1 AND lower(owner_address) = $2 LIMIT 1"
        );
        let found = sqlx::query(&q)
            .bind(&base)
            .bind(&signer)
            .fetch_optional(squid)
            .await?;
        return Ok(found.is_some());
    }

    // Genesis City LAND: owner of any parcel (or its estate) at the place's positions.
    let mut coords: Vec<(i32, i32)> = Vec::new();
    if let Ok(serde_json::Value::Array(arr)) = row.try_get::<serde_json::Value, _>("positions") {
        for p in arr {
            if let Some(s) = p.as_str() {
                if let Some(c) = parse_xy(s) {
                    coords.push(c);
                }
            }
        }
    }
    if coords.is_empty() {
        if let Ok(bp) = row.try_get::<String, _>("base_position") {
            if let Some(c) = parse_xy(&bp) {
                coords.push(c);
            }
        }
    }

    let q = format!(
        "SELECT 1 FROM {schema}.nft p \
         LEFT JOIN {schema}.nft e ON e.category = 'estate' AND e.id = p.search_parcel_estate_id \
         WHERE p.category = 'parcel' AND p.search_parcel_x::int4 = $1 AND p.search_parcel_y::int4 = $2 \
         AND (lower(p.owner_address) = $3 OR lower(e.owner_address) = $3) LIMIT 1"
    );
    for (x, y) in coords {
        let found = sqlx::query(&q)
            .bind(x)
            .bind(y)
            .bind(&signer)
            .fetch_optional(squid)
            .await?;
        if found.is_some() {
            return Ok(true);
        }
    }

    Ok(false)
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
        assert_eq!(parse_xy(""), None);
    }
}
