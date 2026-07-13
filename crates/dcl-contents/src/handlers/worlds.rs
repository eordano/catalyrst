use std::collections::BTreeSet;

use axum::extract::{Path, State};
use axum::Json;

use crate::errors::{ApiError, ApiResult};
use crate::registry::RegistryAppState;
use crate::types::{Coordinate, WorldManifest};

pub async fn get_world_manifest(
    State(state): State<RegistryAppState>,
    Path(world_name): Path<String>,
) -> ApiResult<Json<WorldManifest>> {
    if !is_world_name_valid(&world_name) {
        return Err(ApiError::bad_request("A valid world name is required"));
    }

    let ents = state.content.resolve_world(&world_name).await?;
    let denylist = state.world_policy.denylist().await?;

    let mut occupied_set: BTreeSet<String> = BTreeSet::new();
    let mut matched = false;
    let mut base_coordinate: Option<String> = None;
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
        if base_coordinate.is_none() {
            base_coordinate = scene_base_coordinate(&ent.metadata);
        }
        for p in parcels(&ent.metadata) {
            occupied_set.insert(p);
        }
    }

    if !matched {
        return Err(ApiError::not_found("world not found"));
    }

    let occupied: Vec<String> = occupied_set.into_iter().collect();
    let total = occupied.len();

    let spawn = compute_spawn(&state, &world_name, &occupied, base_coordinate.as_deref()).await?;

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
    state: &RegistryAppState,
    world_name: &str,
    occupied: &[String],
    base_coordinate: Option<&str>,
) -> Result<Coordinate, ApiError> {
    if occupied.is_empty() {
        return Ok(Coordinate { x: 0, y: 0 });
    }
    let bounds = bounds_from_parcels(occupied);
    if let Some((x, y)) = state.world_policy.spawn_override(world_name).await? {
        let c = Coordinate { x, y };
        if in_bounds(&c, &bounds) {
            return Ok(c);
        }
    }
    Ok(auto_spawn(base_coordinate, occupied))
}

fn auto_spawn(base_coordinate: Option<&str>, occupied: &[String]) -> Coordinate {
    if let Some(coord) = base_coordinate.and_then(parse_coordinate) {
        return coord;
    }
    centroid_parcel(occupied)
}

fn scene_base_coordinate(metadata: &serde_json::Value) -> Option<String> {
    let scene = metadata.get("scene");
    if let Some(base) = scene.and_then(|s| s.get("base")).and_then(|b| b.as_str()) {
        if !base.trim().is_empty() {
            return Some(base.to_string());
        }
    }
    scene
        .and_then(|s| s.get("parcels"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
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
        let c = centroid_parcel(&v(&["0,0", "4,0", "2,2"]));
        assert_eq!((c.x, c.y), (2, 2));

        let c = centroid_parcel(&v(&["7,-3"]));
        assert_eq!((c.x, c.y), (7, -3));

        let c = centroid_parcel(&[]);
        assert_eq!((c.x, c.y), (0, 0));

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

    #[test]
    fn scene_base_coordinate_prefers_base_then_first_parcel() {
        let md = serde_json::json!({"scene": {"base": "10,20", "parcels": ["11,21", "10,20"]}});
        assert_eq!(scene_base_coordinate(&md).as_deref(), Some("10,20"));

        let md = serde_json::json!({"scene": {"parcels": ["3,4", "5,6"]}});
        assert_eq!(scene_base_coordinate(&md).as_deref(), Some("3,4"));

        let md = serde_json::json!({"scene": {"base": "  ", "parcels": ["7,8"]}});
        assert_eq!(scene_base_coordinate(&md).as_deref(), Some("7,8"));

        assert_eq!(
            scene_base_coordinate(&serde_json::json!({"scene": {}})),
            None
        );
        assert_eq!(scene_base_coordinate(&serde_json::json!({})), None);
    }

    mod manifest {
        use std::collections::HashSet;
        use std::sync::Arc;

        use axum::extract::{Path, State};
        use axum::response::IntoResponse;
        use serde_json::json;

        use super::super::get_world_manifest;
        use crate::registry::testutil::{entity, open_state, state_with, StubPolicy};
        use crate::types::ActiveEntity;

        fn world_scene(id: &str, name: &str) -> ActiveEntity {
            entity(
                id,
                "scene",
                &[name],
                json!({
                    "worldConfiguration": {"name": name},
                    "scene": {"base": "1,1", "parcels": ["1,1", "1,2"]},
                }),
            )
        }

        fn tmp_root(tag: &str) -> std::path::PathBuf {
            std::env::temp_dir().join(format!("dclc_worlds_{tag}_{}", std::process::id()))
        }

        #[tokio::test]
        async fn happy_path_returns_occupied_and_base_spawn() {
            let tmp = tmp_root("happy");
            let state = open_state(vec![world_scene("bafkworldhappy", "myworld.dcl.eth")], &tmp);

            let axum::Json(m) =
                get_world_manifest(State(state), Path("myworld.dcl.eth".to_string()))
                    .await
                    .unwrap();
            assert_eq!(m.occupied, vec!["1,1".to_string(), "1,2".to_string()]);
            assert_eq!((m.spawn_coordinate.x, m.spawn_coordinate.y), (1, 1));
            assert_eq!(m.total, 2);
        }

        #[tokio::test]
        async fn denylisted_scene_yields_404() {
            let tmp = tmp_root("deny");
            let state = state_with(
                vec![world_scene("bafkworlddenied", "myworld.dcl.eth")],
                Arc::new(StubPolicy {
                    denylist: HashSet::from(["bafkworlddenied".to_string()]),
                    spawn: None,
                }),
                &tmp,
            );

            let err = get_world_manifest(State(state), Path("myworld.dcl.eth".to_string()))
                .await
                .err()
                .unwrap();
            assert_eq!(err.into_response().status().as_u16(), 404);
        }

        #[tokio::test]
        async fn unknown_world_404_and_invalid_name_400() {
            let tmp = tmp_root("miss");
            let state = open_state(Vec::new(), &tmp);

            let missing = get_world_manifest(
                State(state.clone()),
                Path("nosuchworld.dcl.eth".to_string()),
            )
            .await
            .err()
            .unwrap();
            assert_eq!(missing.into_response().status().as_u16(), 404);

            let invalid = get_world_manifest(State(state), Path("not_a_world".to_string()))
                .await
                .err()
                .unwrap();
            assert_eq!(invalid.into_response().status().as_u16(), 400);
        }

        #[tokio::test]
        async fn spawn_override_applies_only_in_bounds() {
            let tmp = tmp_root("spawn");
            let mk = |spawn| {
                state_with(
                    vec![world_scene("bafkworldspawn", "myworld.dcl.eth")],
                    Arc::new(StubPolicy {
                        denylist: HashSet::new(),
                        spawn,
                    }),
                    &tmp,
                )
            };

            let axum::Json(m) =
                get_world_manifest(State(mk(Some((1, 2)))), Path("myworld.dcl.eth".to_string()))
                    .await
                    .unwrap();
            assert_eq!((m.spawn_coordinate.x, m.spawn_coordinate.y), (1, 2));

            let axum::Json(m) =
                get_world_manifest(State(mk(Some((9, 9)))), Path("myworld.dcl.eth".to_string()))
                    .await
                    .unwrap();
            assert_eq!((m.spawn_coordinate.x, m.spawn_coordinate.y), (1, 1));
        }
    }

    #[test]
    fn auto_spawn_prefers_base_over_centroid() {
        let occupied = v(&["0,0", "4,0", "2,2"]);

        let c = auto_spawn(Some("0,0"), &occupied);
        assert_eq!((c.x, c.y), (0, 0));

        let c = auto_spawn(Some("-5,7"), &occupied);
        assert_eq!((c.x, c.y), (-5, 7));

        let c = auto_spawn(None, &occupied);
        assert_eq!((c.x, c.y), (2, 2));

        let c = auto_spawn(Some("my-world.dcl.eth"), &occupied);
        assert_eq!((c.x, c.y), (2, 2));
    }
}
