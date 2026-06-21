use sqlx::Row;

use crate::http::ApiError;
use crate::AppState;

const SQUID_SCHEMA: &str = "squid_marketplace";

fn parse_xy(s: &str) -> Option<(i32, i32)> {
    let mut it = s.splitn(2, ',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
}

pub async fn is_scene_owner_or_admin(
    state: &AppState,
    place_id: &str,
    signer: &str,
) -> Result<bool, ApiError> {
    let signer = signer.to_lowercase();

    if state.scene_admin.is_admin(place_id, &signer).await? {
        return Ok(true);
    }

    let (Some(places), Some(squid)) = (state.places_pool.as_ref(), state.dapps_pool.as_ref())
    else {
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
        return Ok(false);
    };

    let schema = SQUID_SCHEMA;
    let is_world: bool = row.try_get("world").unwrap_or(false);

    if is_world {
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
        let found = sqlx::query(sqlx::AssertSqlSafe(q))
            .bind(&base)
            .bind(&signer)
            .fetch_optional(squid)
            .await?;
        return Ok(found.is_some());
    }

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
        let found = sqlx::query(sqlx::AssertSqlSafe(q.as_str()))
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
