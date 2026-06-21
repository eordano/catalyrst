use std::collections::BTreeSet;

use axum::extract::{Path, State};
use axum::Json;

use crate::http::errors::{ApiError, ApiResult};
use crate::types::{Coordinate, WorldManifest};
use crate::AppState;

pub async fn get_world_manifest(
    State(state): State<AppState>,
    Path(world_name): Path<String>,
) -> ApiResult<Json<WorldManifest>> {
    if !is_world_name_valid(&world_name) {
        return Err(ApiError::bad_request("A valid world name is required"));
    }

    let ents = state
        .content
        .resolve_pointers(std::slice::from_ref(&world_name))
        .await?;
    let denylist = state.registry.denylist_set().await?;

    let mut occupied_set: BTreeSet<String> = BTreeSet::new();
    let mut matched = false;
    for ent in &ents {
        if ent.entity_type != "scene" {
            continue;
        }
        if ent.world_name() != Some(world_name.as_str()) {
            continue;
        }
        if denylist.contains(&ent.entity_id) {
            continue;
        }

        let m = state.manifests.get(&ent.entity_id).await;
        if !m.registry_status(true).is_servable() {
            continue;
        }
        matched = true;
        for p in parcels(&ent.metadata) {
            occupied_set.insert(p);
        }
    }

    if !matched {
        return Err(ApiError::not_found("world not found"));
    }

    let occupied: Vec<String> = occupied_set.into_iter().collect();
    let total = occupied.len();

    let spawn = compute_spawn(&state, &world_name, &occupied).await?;

    Ok(Json(WorldManifest {
        occupied,
        spawn_coordinate: spawn,
        total,
    }))
}

fn is_world_name_valid(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(mut head) = lower.strip_suffix(".eth") else {
        return false;
    };

    if let Some(h) = head.strip_suffix(".dcl") {
        head = h;
    }
    !head.is_empty() && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

async fn compute_spawn(
    state: &AppState,
    world_name: &str,
    occupied: &[String],
) -> Result<Coordinate, sqlx::Error> {
    if occupied.is_empty() {
        return Ok(Coordinate { x: 0, y: 0 });
    }
    let bounds = bounds_from_parcels(occupied);
    if let Some((x, y)) = state.registry.world_spawn(world_name).await? {
        let c = Coordinate { x, y };
        if in_bounds(&c, &bounds) {
            return Ok(c);
        }
    }
    Ok(centroid_parcel(occupied))
}

fn bounds_from_parcels(parcels: &[String]) -> (i64, i64, i64, i64) {
    let mut it = parcels.iter().filter_map(|p| parse_coordinate(p));
    let Some(first) = it.next() else {
        return (0, 0, 0, 0);
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    for c in it {
        min_x = min_x.min(c.x);
        min_y = min_y.min(c.y);
        max_x = max_x.max(c.x);
        max_y = max_y.max(c.y);
    }
    (min_x, min_y, max_x, max_y)
}

fn in_bounds(c: &Coordinate, b: &(i64, i64, i64, i64)) -> bool {
    c.x >= b.0 && c.x <= b.2 && c.y >= b.1 && c.y <= b.3
}

fn centroid_parcel(parcels: &[String]) -> Coordinate {
    let coords: Vec<Coordinate> = parcels.iter().filter_map(|p| parse_coordinate(p)).collect();
    if coords.is_empty() {
        return Coordinate { x: 0, y: 0 };
    }
    let n = coords.len() as f64;
    let cx = coords.iter().map(|c| c.x as f64).sum::<f64>() / n;
    let cy = coords.iter().map(|c| c.y as f64).sum::<f64>() / n;
    let mut best = coords[0];
    let mut best_d = f64::INFINITY;
    for c in &coords {
        let d = (c.x as f64 - cx).powi(2) + (c.y as f64 - cy).powi(2);
        if d < best_d {
            best_d = d;
            best = *c;
        }
    }
    best
}

fn parse_coordinate(s: &str) -> Option<Coordinate> {
    let mut it = s.split(',');
    let x = it.next()?.trim().parse().ok()?;
    let y = it.next()?.trim().parse().ok()?;
    Some(Coordinate { x, y })
}

fn parcels(metadata: &serde_json::Value) -> Vec<String> {
    metadata
        .get("scene")
        .and_then(|s| s.get("parcels"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn world_name_validation_matches_upstream_regex() {
        // upstream: /^[a-zA-Z0-9-]+(\.dcl)?\.eth$/
        for ok in [
            "foo.eth",
            "foo.dcl.eth",
            "my-world.dcl.eth",
            "UPPER.eth",
            "a.eth",
            "1.dcl.eth",
        ] {
            assert!(is_world_name_valid(ok), "should be valid: {ok}");
        }
        for bad in [
            "invalid",
            "notaworld.com",
            "",
            "bad_name.eth",
            "foo.bar.eth",
            ".eth",
            ".dcl.eth",
            "foo.dcl.dcl.eth",
            "foo..eth",
        ] {
            assert!(!is_world_name_valid(bad), "should be invalid: {bad}");
        }
    }

    #[test]
    fn parse_coordinate_handles_signs_whitespace_and_garbage() {
        assert_eq!(parse_coordinate("1,2").map(|c| (c.x, c.y)), Some((1, 2)));
        assert_eq!(
            parse_coordinate("-5,-9").map(|c| (c.x, c.y)),
            Some((-5, -9))
        );
        assert_eq!(
            parse_coordinate(" 3 , 4 ").map(|c| (c.x, c.y)),
            Some((3, 4))
        );
        assert!(parse_coordinate("x,y").is_none());
        assert!(parse_coordinate("1").is_none());
        assert!(parse_coordinate("").is_none());
    }

    #[test]
    fn bounds_are_min_max_over_all_parcels() {
        assert_eq!(
            bounds_from_parcels(&v(&["0,0", "10,4", "-2,6"])),
            (-2, 0, 10, 6)
        );
        assert_eq!(bounds_from_parcels(&v(&["5,5"])), (5, 5, 5, 5));
        assert_eq!(bounds_from_parcels(&[]), (0, 0, 0, 0));
    }

    #[test]
    fn in_bounds_is_inclusive_rectangle() {
        let b = (-2, 0, 10, 6);
        assert!(in_bounds(&Coordinate { x: -2, y: 0 }, &b));
        assert!(in_bounds(&Coordinate { x: 10, y: 6 }, &b));
        assert!(in_bounds(&Coordinate { x: 4, y: 3 }, &b));
        assert!(!in_bounds(&Coordinate { x: -3, y: 3 }, &b));
        assert!(!in_bounds(&Coordinate { x: 4, y: 7 }, &b));
    }

    #[test]
    fn centroid_returns_occupied_parcel_closest_to_average() {
        // centroid of {(0,0),(4,0),(2,2)} = (2, 0.667); nearest occupied parcel is (2,2)
        let c = centroid_parcel(&v(&["0,0", "4,0", "2,2"]));
        assert_eq!((c.x, c.y), (2, 2));
        // single parcel -> itself
        let c = centroid_parcel(&v(&["7,-3"]));
        assert_eq!((c.x, c.y), (7, -3));
        // empty -> (0,0)
        let c = centroid_parcel(&[]);
        assert_eq!((c.x, c.y), (0, 0));
        // result is always one of the occupied parcels (closest-in-set, not raw centroid)
        let set = v(&["0,0", "1,0", "5,5"]);
        let c = centroid_parcel(&set);
        assert!(set.contains(&format!("{},{}", c.x, c.y)));
    }

    #[test]
    fn parcels_extracts_scene_parcels_array() {
        let md = serde_json::json!({"scene": {"parcels": ["0,0", "0,1"]}});
        assert_eq!(parcels(&md), v(&["0,0", "0,1"]));
        assert_eq!(parcels(&serde_json::json!({})), Vec::<String>::new());
    }
}
