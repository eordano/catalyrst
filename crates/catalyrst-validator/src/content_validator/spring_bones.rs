use std::collections::HashSet;

use crate::error::ValidationResponse;
use crate::types::*;

const SPRING_BONE_NAME_TOKEN: &str = "springbone";

fn is_spring_bone_name(name: &str) -> bool {
    name.to_lowercase().contains(SPRING_BONE_NAME_TOKEN)
}

pub fn validate_spring_bones_metadata(entity: &Entity) -> ValidationResponse {
    let metadata = match &entity.metadata {
        Some(m) => m,
        None => return ValidationResponse::Ok,
    };

    let spring_bones = match metadata.get("data").and_then(|d| d.get("springBones")) {
        Some(sb) if !sb.is_null() => sb,
        _ => return ValidationResponse::Ok,
    };

    let schema_errors = validate_spring_bones_schema(spring_bones);
    if !schema_errors.is_empty() {
        return ValidationResponse::failed(schema_errors);
    }

    let mut active_hashes: HashSet<&str> = HashSet::new();
    if let Some(reps) = metadata
        .get("data")
        .and_then(|d| d.get("representations"))
        .and_then(|r| r.as_array())
    {
        for rep in reps {
            if let Some(main_file) = rep.get("mainFile").and_then(|f| f.as_str()) {
                if let Some(entry) = entity.content.iter().find(|c| c.file == main_file) {
                    active_hashes.insert(entry.hash.as_str());
                }
            }
        }
    }

    let mut errors: Vec<String> = Vec::new();
    if let Some(models) = spring_bones.get("models").and_then(|m| m.as_object()) {
        for (model_hash, bones) in models {
            if !active_hashes.contains(model_hash.as_str()) {
                errors.push(format!(
                    "springBones.models key {model_hash} does not match any current \
                     representation hash"
                ));
            }
            if let Some(bones) = bones.as_object() {
                for bone_name in bones.keys() {
                    if !is_spring_bone_name(bone_name) {
                        errors.push(format!(
                            "Bone name {bone_name} in model {model_hash} does not follow \
                             the spring bone naming convention"
                        ));
                    }
                }
            }
        }
    }

    ValidationResponse::from_errors(errors)
}

fn validate_spring_bones_schema(spring_bones: &serde_json::Value) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    let obj = match spring_bones.as_object() {
        Some(o) => o,
        None => {
            errors.push("springBones must be object".to_string());
            return errors;
        }
    };

    for key in obj.keys() {
        if key != "version" && key != "models" {
            errors.push(format!(
                "springBones must NOT have additional properties ({key})"
            ));
        }
    }

    match obj.get("version") {
        None => errors.push("springBones must have required property 'version'".to_string()),
        Some(v) => {
            if v.as_i64() != Some(1) {
                errors.push(
                    "springBones/version must be equal to one of the allowed values".to_string(),
                );
            }
        }
    }

    match obj.get("models") {
        None => errors.push("springBones must have required property 'models'".to_string()),
        Some(models) => match models.as_object() {
            None => errors.push("springBones/models must be object".to_string()),
            Some(models) => {
                for (model_hash, bones) in models {
                    let model_path = format!("springBones/models/{model_hash}");
                    match bones.as_object() {
                        None => errors.push(format!("{model_path} must be object")),
                        Some(bones) => {
                            for (bone_name, params) in bones {
                                validate_spring_bone_params(
                                    &format!("{model_path}/{bone_name}"),
                                    params,
                                    &mut errors,
                                );
                            }
                        }
                    }
                }
            }
        },
    }

    errors
}

fn validate_spring_bone_params(path: &str, params: &serde_json::Value, errors: &mut Vec<String>) {
    let obj = match params.as_object() {
        Some(o) => o,
        None => {
            errors.push(format!("{path} must be object"));
            return;
        }
    };

    for required in ["stiffness", "gravityPower", "gravityDir", "drag"] {
        if !obj.contains_key(required) {
            errors.push(format!("{path} must have required property '{required}'"));
        }
    }

    if let Some(v) = obj.get("stiffness") {
        check_number_range(v, &format!("{path}/stiffness"), 0.0, 4.0, errors);
    }
    if let Some(v) = obj.get("gravityPower") {
        check_number_range(v, &format!("{path}/gravityPower"), 0.0, 2.0, errors);
    }
    if let Some(v) = obj.get("drag") {
        check_number_range(v, &format!("{path}/drag"), 0.0, 1.0, errors);
    }

    if let Some(v) = obj.get("gravityDir") {
        let gd_path = format!("{path}/gravityDir");
        match v.as_array() {
            None => errors.push(format!("{gd_path} must be array")),
            Some(items) => {
                if items.len() < 3 {
                    errors.push(format!("{gd_path} must NOT have fewer than 3 items"));
                } else if items.len() > 3 {
                    errors.push(format!("{gd_path} must NOT have more than 3 items"));
                }
                for (i, item) in items.iter().take(3).enumerate() {
                    check_number_range(item, &format!("{gd_path}/{i}"), -10.0, 10.0, errors);
                }
            }
        }
    }

    if let Some(v) = obj.get("center") {
        if !v.is_string() && !v.is_null() {
            errors.push(format!("{path}/center must be string"));
        }
    }

    if let Some(v) = obj.get("isRoot") {
        if !v.is_boolean() && !v.is_null() {
            errors.push(format!("{path}/isRoot must be boolean"));
        }
    }
}

fn check_number_range(
    value: &serde_json::Value,
    path: &str,
    minimum: f64,
    maximum: f64,
    errors: &mut Vec<String>,
) {
    match value.as_f64() {
        None => errors.push(format!("{path} must be number")),
        Some(n) => {
            if n < minimum {
                errors.push(format!("{path} must be >= {minimum}"));
            } else if n > maximum {
                errors.push(format!("{path} must be <= {maximum}"));
            }
        }
    }
}

#[cfg(test)]
mod spring_bones_tests {
    use super::*;
    use serde_json::{json, Map, Value};

    const FILE_HASH: &str = "bafkreialsvt77jvpy673cnugp5ggnxfaalfncufweayuk3jbxskh3pelkm";
    const FILE2_HASH: &str = "bafkreigreflbn4w3a36rgg2ywlhf2asebqlsd4skg5q5djpklcdcjkbjvi";

    fn valid_bone_params() -> Value {
        json!({
            "stiffness": 2,
            "gravityPower": 0,
            "gravityDir": [0, -1, 0],
            "drag": 0.5,
            "isRoot": true
        })
    }

    fn bones_map(entries: &[(&str, Value)]) -> Value {
        let mut m = Map::new();
        for (name, params) in entries {
            m.insert((*name).to_string(), params.clone());
        }
        Value::Object(m)
    }

    fn models_map(entries: &[(&str, Value)]) -> Value {
        let mut m = Map::new();
        for (hash, bones) in entries {
            m.insert((*hash).to_string(), bones.clone());
        }
        Value::Object(m)
    }

    fn wearable_with_spring_bones(spring_bones: Value) -> Entity {
        let mut data = json!({
            "category": "hat",
            "tags": [],
            "representations": [
                {
                    "bodyShapes": ["urn:decentraland:off-chain:base-avatars:BaseMale"],
                    "mainFile": "file1",
                    "contents": ["file1"],
                    "overrideHides": [],
                    "overrideReplaces": []
                }
            ]
        });
        data.as_object_mut()
            .unwrap()
            .insert("springBones".to_string(), spring_bones);

        Entity {
            id: "bafkrei-entity".to_string(),
            entity_type: EntityType::Wearable,
            pointers: vec!["urn:decentraland:matic:collections-v2:0xabc:0".to_string()],
            timestamp: 1_700_000_000_000,
            content: vec![
                ContentMapping {
                    file: "file1".to_string(),
                    hash: FILE_HASH.to_string(),
                },
                ContentMapping {
                    file: "file2".to_string(),
                    hash: FILE2_HASH.to_string(),
                },
            ],
            version: "v3".to_string(),
            metadata: Some(json!({ "name": "Test Wearable", "data": data })),
        }
    }

    fn assert_error_contains(resp: &ValidationResponse, needle: &str) {
        let errors = resp.errors().unwrap_or_default();
        assert!(
            errors.iter().any(|e| e.contains(needle)),
            "expected an error containing {needle:?}, got {errors:?}"
        );
    }

    fn assert_has_error(resp: &ValidationResponse, expected: &str) {
        let errors = resp.errors().unwrap_or_default();
        assert!(
            errors.iter().any(|e| e == expected),
            "expected error {expected:?}, got {errors:?}"
        );
    }

    #[test]
    fn springbones_absent_passes() {
        let mut entity = wearable_with_spring_bones(Value::Null);
        entity
            .metadata
            .as_mut()
            .unwrap()
            .get_mut("data")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("springBones");
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn springbones_null_passes() {
        let entity = wearable_with_spring_bones(Value::Null);
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn empty_models_passes() {
        let entity = wearable_with_spring_bones(json!({ "version": 1, "models": {} }));
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn representation_hash_with_canonical_bone_name_passes() {
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(
                FILE_HASH,
                bones_map(&[("Hair_springBone_L", valid_bone_params())]),
            )]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn isroot_omitted_passes() {
        let mut params = valid_bone_params();
        params.as_object_mut().unwrap().remove("isRoot");
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn center_as_string_passes() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("center".to_string(), json!("Avatar_Hips"));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn two_representations_sharing_glb_hash_passes() {
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(
                FILE_HASH,
                bones_map(&[("Hair_springBone", valid_bone_params())]),
            )]),
        });
        let entity = Entity {
            id: "bafkrei-entity".to_string(),
            entity_type: EntityType::Wearable,
            pointers: vec!["urn:decentraland:matic:collections-v2:0xabc:0".to_string()],
            timestamp: 1_700_000_000_000,
            content: vec![
                ContentMapping {
                    file: "male/shared.glb".to_string(),
                    hash: FILE_HASH.to_string(),
                },
                ContentMapping {
                    file: "female/shared.glb".to_string(),
                    hash: FILE_HASH.to_string(),
                },
            ],
            version: "v3".to_string(),
            metadata: Some(json!({
                "name": "Test Wearable",
                "data": {
                    "category": "hat",
                    "tags": [],
                    "representations": [
                        { "mainFile": "male/shared.glb", "contents": ["male/shared.glb"] },
                        { "mainFile": "female/shared.glb", "contents": ["female/shared.glb"] }
                    ],
                    "springBones": spring_bones
                }
            })),
        };
        assert!(validate_spring_bones_metadata(&entity).is_ok());
    }

    #[test]
    fn models_keyed_by_filename_reports_unmatched_hash() {
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(
                "male/AnimeLong.glb",
                bones_map(&[("Hair_springBone_L", valid_bone_params())]),
            )]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_has_error(
            &result,
            "springBones.models key male/AnimeLong.glb does not match any current representation hash",
        );
    }

    #[test]
    fn models_keyed_by_stale_hash_reports_unmatched_hash() {
        let stale = "bafkreistaleshashstalehashstalehashstalehashstalehashstalehashstale";
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(
                stale,
                bones_map(&[("Hair_springBone_L", valid_bone_params())]),
            )]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_has_error(
            &result,
            &format!(
                "springBones.models key {stale} does not match any current representation hash"
            ),
        );
    }

    #[test]
    fn non_canonical_bone_name_reports_invalid_name() {
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_001", valid_bone_params())]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_has_error(
            &result,
            &format!(
                "Bone name Hair_001 in model {FILE_HASH} does not follow the spring bone \
                 naming convention"
            ),
        );
    }

    #[test]
    fn version_not_one_reports_version_error() {
        let spring_bones = json!({
            "version": 10,
            "models": models_map(&[(
                FILE_HASH,
                bones_map(&[("Hair_springBone_L", valid_bone_params())]),
            )]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "version");
    }

    #[test]
    fn missing_version_reports_version_error() {
        let spring_bones = json!({
            "models": models_map(&[(
                FILE_HASH,
                bones_map(&[("Hair_springBone_L", valid_bone_params())]),
            )]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "version");
    }

    #[test]
    fn stiffness_out_of_range_reports_stiffness_error() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("stiffness".to_string(), json!(6));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "stiffness");
    }

    #[test]
    fn gravity_power_out_of_range_reports_gravity_power_error() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("gravityPower".to_string(), json!(11));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "gravityPower");
    }

    #[test]
    fn drag_out_of_range_reports_drag_error() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("drag".to_string(), json!(1.5));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "drag");
    }

    #[test]
    fn gravity_dir_wrong_length_reports_gravity_dir_error() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("gravityDir".to_string(), json!([0, -1]));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "gravityDir");
    }

    #[test]
    fn gravity_dir_non_numeric_reports_gravity_dir_error() {
        let mut params = valid_bone_params();
        params
            .as_object_mut()
            .unwrap()
            .insert("gravityDir".to_string(), json!([0, -1, "x"]));
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[(FILE_HASH, bones_map(&[("Hair_springBone_L", params)]))]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "gravityDir");
    }

    #[test]
    fn models_null_reports_models_error() {
        let entity = wearable_with_spring_bones(json!({ "version": 1, "models": Value::Null }));
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_error_contains(&result, "models");
    }

    #[test]
    fn multiple_invalid_entries_accumulate() {
        let stale = "bafkreistaleshashstale";
        let spring_bones = json!({
            "version": 1,
            "models": models_map(&[
                (stale, bones_map(&[("Hair_springBone_L", valid_bone_params())])),
                (FILE_HASH, bones_map(&[("Hair_001", valid_bone_params())])),
            ]),
        });
        let entity = wearable_with_spring_bones(spring_bones);
        let result = validate_spring_bones_metadata(&entity);
        assert!(!result.is_ok());
        assert_has_error(
            &result,
            &format!(
                "springBones.models key {stale} does not match any current representation hash"
            ),
        );
        assert_has_error(
            &result,
            &format!(
                "Bone name Hair_001 in model {FILE_HASH} does not follow the spring bone \
                 naming convention"
            ),
        );
    }
}
