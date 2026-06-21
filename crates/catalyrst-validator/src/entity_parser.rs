use serde::Deserialize;

use crate::error::ValidatorError;
use crate::types::{ContentMapping, Entity, EntityType};

#[derive(Deserialize)]
struct RawEntity {
    #[serde(rename = "type")]
    entity_type: Option<serde_json::Value>,
    pointers: Option<serde_json::Value>,
    timestamp: Option<serde_json::Value>,
    content: Option<serde_json::Value>,
    version: Option<String>,
    metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawContentEntry {
    file: Option<serde_json::Value>,
    hash: Option<serde_json::Value>,
}

pub fn parse_entity_from_bytes(buffer: &[u8], id: &str) -> Result<Entity, ValidatorError> {
    let raw: RawEntity = serde_json::from_slice(buffer).map_err(|e| {
        ValidatorError::EntityParse(format!(
            "Failed to parse the entity file. Please make sure that it is a valid json. {e}"
        ))
    })?;

    let entity_type = validate_entity_type(&raw)?;
    let pointers = validate_pointers(&raw)?;
    let timestamp = validate_timestamp(&raw)?;
    let content = validate_content(&raw)?;

    let version = raw.version.unwrap_or_else(|| "v3".to_string());
    let normalised_pointers: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();

    Ok(Entity {
        id: id.to_string(),
        entity_type,
        pointers: normalised_pointers,
        timestamp,
        content,
        version,
        metadata: raw.metadata,
    })
}

fn validate_entity_type(raw: &RawEntity) -> Result<EntityType, ValidatorError> {
    let type_val = raw.entity_type.as_ref().ok_or_else(|| {
        ValidatorError::EntityParse(format!(
            "Please set a valid type. It must be one of {:?}. We got 'undefined'",
            EntityType::ALL
        ))
    })?;

    let type_str = type_val.as_str().ok_or_else(|| {
        ValidatorError::EntityParse(format!(
            "Please set a valid type. It must be one of {:?}. We got '{type_val}'",
            EntityType::ALL
        ))
    })?;

    EntityType::parse(type_str).ok_or_else(|| {
        ValidatorError::EntityParse(format!(
            "Please set a valid type. It must be one of {:?}. We got '{type_str}'",
            EntityType::ALL
        ))
    })
}

fn validate_pointers(raw: &RawEntity) -> Result<Vec<String>, ValidatorError> {
    let val = raw
        .pointers
        .as_ref()
        .ok_or_else(|| ValidatorError::EntityParse("Please set valid pointers".to_string()))?;

    let arr = val
        .as_array()
        .ok_or_else(|| ValidatorError::EntityParse("Please set valid pointers".to_string()))?;

    let mut pointers = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item
            .as_str()
            .ok_or_else(|| ValidatorError::EntityParse("Please set valid pointers".to_string()))?;
        pointers.push(s.to_string());
    }

    Ok(pointers)
}

fn checked_f64_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    if v < i64::MIN as f64 || v > i64::MAX as f64 {
        return None;
    }
    Some(v as i64)
}

fn validate_timestamp(raw: &RawEntity) -> Result<i64, ValidatorError> {
    let val = raw.timestamp.as_ref().ok_or_else(|| {
        ValidatorError::EntityParse("Please set a valid timestamp. We got undefined".to_string())
    })?;

    if let Some(n) = val.as_i64() {
        return Ok(n);
    }
    if let Some(n) = val.as_f64() {
        if let Some(i) = checked_f64_to_i64(n) {
            return Ok(i);
        }
    }

    Err(ValidatorError::EntityParse(format!(
        "Please set a valid timestamp. We got {val}"
    )))
}

fn validate_content(raw: &RawEntity) -> Result<Vec<ContentMapping>, ValidatorError> {
    let val = match &raw.content {
        None => return Ok(Vec::new()),
        Some(v) if v.is_null() => return Ok(Vec::new()),
        Some(v) => v,
    };

    let arr = val
        .as_array()
        .ok_or_else(|| ValidatorError::EntityParse("Expected an array as content".to_string()))?;

    if arr.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::with_capacity(arr.len());
    for item in arr {
        let entry: RawContentEntry = serde_json::from_value(item.clone()).map_err(|_| {
            ValidatorError::EntityParse(
                "Content must contain a file name and a file hash".to_string(),
            )
        })?;

        let file = entry
            .file
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ValidatorError::EntityParse(
                    "Content must contain a file name and a file hash".to_string(),
                )
            })?
            .to_string();

        let hash = entry
            .hash
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ValidatorError::EntityParse(
                    "Please make sure that all file names and a file hashes are valid strings"
                        .to_string(),
                )
            })?
            .to_string();

        result.push(ContentMapping { file, hash });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_entity() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1700000000000,
            "content": [{"file": "scene.json", "hash": "bafkreiaaaa"}]
        }"#;

        let entity = parse_entity_from_bytes(json.as_bytes(), "bafkreitest").unwrap();
        assert_eq!(entity.entity_type, EntityType::Scene);
        assert_eq!(entity.pointers, vec!["0,0"]);
        assert_eq!(entity.timestamp, 1700000000000);
        assert_eq!(entity.content.len(), 1);
        assert_eq!(entity.version, "v3");
        assert_eq!(entity.id, "bafkreitest");
    }

    #[test]
    fn parse_normalises_pointers_to_lowercase() {
        let json = r#"{
            "type": "profile",
            "pointers": ["0xAbCdEf1234567890AbCdEf1234567890ABCDEF12"],
            "timestamp": 1700000000000
        }"#;

        let entity = parse_entity_from_bytes(json.as_bytes(), "id").unwrap();
        assert_eq!(
            entity.pointers,
            vec!["0xabcdef1234567890abcdef1234567890abcdef12"]
        );
    }

    #[test]
    fn reject_missing_type() {
        let json = r#"{"pointers": ["0,0"], "timestamp": 1}"#;
        assert!(parse_entity_from_bytes(json.as_bytes(), "id").is_err());
    }

    #[test]
    fn reject_invalid_json() {
        assert!(parse_entity_from_bytes(b"not json", "id").is_err());
    }

    #[test]
    fn reject_non_array_content() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1,
            "content": "invalid"
        }"#;
        assert!(parse_entity_from_bytes(json.as_bytes(), "id").is_err());
    }

    #[test]
    fn reject_content_entry_with_hash_but_no_file() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1,
            "content": [{"hash": "bafkreiaaaa"}]
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Content must contain a file name and a file hash"),
            "Expected 'Content must contain a file name and a file hash', got: {msg}"
        );
    }

    #[test]
    fn reject_content_entry_with_file_but_no_hash() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1,
            "content": [{"file": "scene.json"}]
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file name") || msg.contains("file hash"),
            "Expected content validation error, got: {msg}"
        );
    }

    #[test]
    fn reject_pointers_as_string_instead_of_array() {
        let json = r#"{
            "type": "scene",
            "pointers": "invalidPointers",
            "timestamp": 1
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("valid pointers"),
            "Expected 'valid pointers' error, got: {msg}"
        );
    }

    #[test]
    fn reject_timestamp_as_string_instead_of_number() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": "invalidTimestamp"
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("valid timestamp"),
            "Expected 'valid timestamp' error, got: {msg}"
        );
    }

    #[test]
    fn reject_content_file_is_number_not_string() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1,
            "content": [{"file": 1234, "hash": "bafkreiaaaa"}]
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file name") && msg.contains("file hash"),
            "Expected content string validation error, got: {msg}"
        );
    }

    #[test]
    fn reject_content_hash_is_number_not_string() {
        let json = r#"{
            "type": "scene",
            "pointers": ["0,0"],
            "timestamp": 1,
            "content": [{"file": "scene.json", "hash": 1234}]
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("valid strings"),
            "Expected 'valid strings' error, got: {msg}"
        );
    }

    #[test]
    fn reject_pointers_array_with_non_string_elements() {
        let json = r#"{
            "type": "scene",
            "pointers": [1234],
            "timestamp": 1
        }"#;
        let err = parse_entity_from_bytes(json.as_bytes(), "id").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("valid pointers"),
            "Expected 'valid pointers' error, got: {msg}"
        );
    }
}
