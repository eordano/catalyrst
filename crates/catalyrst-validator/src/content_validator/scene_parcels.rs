use std::collections::HashSet;

use serde_json::Value;

use crate::error::ValidationResponse;

const METADATA_ERROR: &str =
    "Scene parcels metadata must be valid, canonical, unique, and include the base parcel.";
const POINTERS_ERROR: &str = "Scene pointers must be unique canonical parcel coordinates.";
const MATCH_ERROR: &str = "The scene parcels must match the entity pointers.";

fn is_canonical_parcel_list(parcels: &[&str]) -> bool {
    !parcels.is_empty()
        && parcels
            .iter()
            .all(|p| catalyrst_types::pointer::is_canonical_pointer(p))
        && parcels.iter().collect::<HashSet<_>>().len() == parcels.len()
}

pub fn validate_scene_parcels_match_pointers(
    metadata: Option<&Value>,
    pointers: &[String],
) -> ValidationResponse {
    let scene = metadata.and_then(|m| m.get("scene"));
    let base = scene.and_then(|s| s.get("base")).and_then(Value::as_str);
    let parcels: Option<Vec<&str>> = scene
        .and_then(|s| s.get("parcels"))
        .and_then(Value::as_array)
        .and_then(|arr| arr.iter().map(Value::as_str).collect());

    let (Some(base), Some(parcels)) = (base, parcels) else {
        return ValidationResponse::fail(METADATA_ERROR.to_string());
    };
    if !is_canonical_parcel_list(&parcels) || !parcels.contains(&base) {
        return ValidationResponse::fail(METADATA_ERROR.to_string());
    }

    let pointer_refs: Vec<&str> = pointers.iter().map(String::as_str).collect();
    if !is_canonical_parcel_list(&pointer_refs) {
        return ValidationResponse::fail(POINTERS_ERROR.to_string());
    }

    let pointer_set: HashSet<&str> = pointer_refs.iter().copied().collect();
    if pointer_set.len() != parcels.len() || parcels.iter().any(|p| !pointer_set.contains(p)) {
        return ValidationResponse::fail(MATCH_ERROR.to_string());
    }

    ValidationResponse::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene_metadata(base: &str, parcels: &[&str]) -> Value {
        json!({ "scene": { "base": base, "parcels": parcels } })
    }

    fn ptrs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn failed_with(response: ValidationResponse, expected: &str) {
        match response {
            ValidationResponse::Failed { errors } => assert_eq!(errors, vec![expected.to_string()]),
            other => panic!("expected failure '{expected}', got {other}"),
        }
    }

    #[test]
    fn matching_base_parcels_and_pointers_pass() {
        let metadata = scene_metadata("0,0", &["0,0"]);
        let response = validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&["0,0"]));
        assert!(response.is_ok());
    }

    #[test]
    fn order_and_negative_coordinates_do_not_matter() {
        let metadata = scene_metadata("-1,5", &["-1,5", "0,5", "1,5"]);
        let response =
            validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&["1,5", "-1,5", "0,5"]));
        assert!(response.is_ok());
    }

    #[test]
    fn large_parcel_sets_have_no_count_limit() {
        let coords: Vec<String> = (0..60)
            .flat_map(|x| (0..60).map(move |y| format!("{x},{y}")))
            .collect();
        let coord_refs: Vec<&str> = coords.iter().map(String::as_str).collect();
        let metadata = scene_metadata("0,0", &coord_refs);
        let response = validate_scene_parcels_match_pointers(Some(&metadata), &coords);
        assert!(response.is_ok());
    }

    #[test]
    fn base_outside_parcels_is_rejected() {
        let metadata = scene_metadata("9,9", &["0,0", "0,1"]);
        let response =
            validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&["0,0", "0,1"]));
        failed_with(response, METADATA_ERROR);
    }

    #[test]
    fn duplicate_parcels_are_rejected() {
        let metadata = scene_metadata("0,0", &["0,0", "0,0"]);
        let response = validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&["0,0"]));
        failed_with(response, METADATA_ERROR);
    }

    #[test]
    fn non_canonical_coordinates_are_rejected() {
        for bad in ["01,2", "-0,0", "0, 1", "1.5,2", "0", "a,b", "1,,2", ""] {
            let metadata = scene_metadata(bad, &[bad]);
            let response = validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&[bad]));
            failed_with(response, METADATA_ERROR);
        }
    }

    #[test]
    fn missing_metadata_or_scene_or_fields_is_rejected() {
        let no_scene = json!({});
        for (metadata, pointers) in [(None, ptrs(&["0,0"])), (Some(&no_scene), ptrs(&["0,0"]))] {
            failed_with(
                validate_scene_parcels_match_pointers(metadata, &pointers),
                METADATA_ERROR,
            );
        }
        let non_string = json!({ "scene": { "base": "0,0", "parcels": ["0,0", 7] } });
        failed_with(
            validate_scene_parcels_match_pointers(Some(&non_string), &ptrs(&["0,0"])),
            METADATA_ERROR,
        );
    }

    #[test]
    fn non_canonical_or_duplicate_pointers_are_rejected() {
        let metadata = scene_metadata("0,0", &["0,0"]);
        failed_with(
            validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&["00,0"])),
            POINTERS_ERROR,
        );
        failed_with(
            validate_scene_parcels_match_pointers(Some(&metadata), &ptrs(&[])),
            POINTERS_ERROR,
        );
        let two = scene_metadata("0,0", &["0,0", "0,1"]);
        failed_with(
            validate_scene_parcels_match_pointers(Some(&two), &ptrs(&["0,0", "0,1", "0,1"])),
            POINTERS_ERROR,
        );
    }

    #[test]
    fn parcel_pointer_set_mismatch_is_rejected() {
        let metadata = scene_metadata("0,0", &["0,0", "0,1"]);
        for pointers in [
            ptrs(&["0,0"]),
            ptrs(&["0,0", "0,2"]),
            ptrs(&["0,0", "0,1", "0,2"]),
        ] {
            failed_with(
                validate_scene_parcels_match_pointers(Some(&metadata), &pointers),
                MATCH_ERROR,
            );
        }
    }
}
