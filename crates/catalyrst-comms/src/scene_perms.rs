use sqlx::Row;

use crate::http::ApiError;
use crate::AppState;

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

    let schema = state.dapps_schema.as_str();
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

    if coords.is_empty() {
        return Ok(false);
    }
    let (xs, ys): (Vec<i32>, Vec<i32>) = coords.into_iter().unzip();
    let q = format!(
        "SELECT 1 FROM {schema}.nft p \
         LEFT JOIN {schema}.nft e ON e.category = 'estate' AND e.id = p.search_parcel_estate_id \
         JOIN unnest($1::int4[], $2::int4[]) AS c(x, y) \
           ON p.search_parcel_x::int4 = c.x AND p.search_parcel_y::int4 = c.y \
         WHERE p.category = 'parcel' \
           AND (lower(p.owner_address) = $3 OR lower(e.owner_address) = $3) LIMIT 1"
    );
    let found = sqlx::query(sqlx::AssertSqlSafe(q))
        .bind(&xs)
        .bind(&ys)
        .bind(&signer)
        .fetch_optional(squid)
        .await?;
    Ok(found.is_some())
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
