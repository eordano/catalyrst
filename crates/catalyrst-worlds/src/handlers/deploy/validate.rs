use std::collections::BTreeMap;

use serde_json::Value;

pub(super) const MAX_ENTITY_FILE_SIZE_BYTES: usize = 5 * 1024 * 1024;

const MIN_PARCEL_COORDINATE: i64 = -150;
const MAX_PARCEL_COORDINATE: i64 = 150;

pub(crate) fn canon_pointer(s: &str) -> String {
    catalyrst_types::pointer::canonicalize_pointer(s)
}

pub(super) fn canon_pointer_set(values: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = values
        .iter()
        .filter_map(|v| v.as_str().map(canon_pointer))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// True iff `values` is a non-empty array of unique, already-canonical parcel coordinate
/// strings -- the shape `@dcl/schemas` `SceneParcels` enforces. Non-canonical spellings and
/// duplicates are rejected rather than normalized, so a deployment is authorized and sized
/// against exactly the literal set of parcels it is placed on.
pub(super) fn is_canonical_parcel_set(values: &[Value]) -> bool {
    if values.is_empty() {
        return false;
    }
    let mut seen = std::collections::HashSet::with_capacity(values.len());
    for v in values {
        match v.as_str() {
            Some(s) if catalyrst_types::pointer::is_canonical_pointer(s) => {
                if !seen.insert(s) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn validate_parcel_in_bounds(parcel: &str) -> Result<(), String> {
    let (x, y) = match catalyrst_types::pointer::parse_pointer(parcel) {
        Some(xy) => xy,
        None => return Err(format!("Invalid coordinate format: {parcel}")),
    };
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&x) {
        return Err(format!(
            "Coordinate X value {x} is out of bounds. Must be between {MIN_PARCEL_COORDINATE} and {MAX_PARCEL_COORDINATE}."
        ));
    }
    if !(MIN_PARCEL_COORDINATE..=MAX_PARCEL_COORDINATE).contains(&y) {
        return Err(format!(
            "Coordinate Y value {y} is out of bounds. Must be between {MIN_PARCEL_COORDINATE} and {MAX_PARCEL_COORDINATE}."
        ));
    }
    Ok(())
}

pub(super) fn extract_auth_chain_from_fields(
    fields: &BTreeMap<String, String>,
) -> Result<Value, String> {
    catalyrst_types::deploy_form::extract_auth_chain_from_fields(fields)?
        .ok_or_else(|| "No auth chain can be derived".to_string())
}

pub(super) fn entity_file_too_large_error() -> String {
    format!(
        "The entity file is too large. The maximum allowed size is {MAX_ENTITY_FILE_SIZE_BYTES} bytes."
    )
}

fn has_uri_scheme(path: &str) -> bool {
    let mut chars = path.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        match c {
            ':' => return true,
            c if c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-') => {}
            _ => return false,
        }
    }
    false
}

fn is_relative_thumbnail_path(path: &str) -> bool {
    if has_uri_scheme(path) || path.starts_with('/') {
        return false;
    }
    let js_ws = |c: char| c.is_whitespace() || c == '\u{FEFF}';
    if path.starts_with(js_ws) || path.ends_with(js_ws) {
        return false;
    }
    if path.chars().any(|c| c.is_control()) {
        return false;
    }
    !path.contains(['<', '>', '"'])
}

pub(super) fn validate_navmap_thumbnail(entity: &Value, errors: &mut Vec<String>) {
    let thumb = match entity
        .get("metadata")
        .and_then(|m| m.get("display"))
        .and_then(|d| d.get("navmapThumbnail"))
    {
        None | Some(Value::Null) => return,
        Some(Value::String(s)) if s.is_empty() => return,
        Some(Value::Bool(false)) => return,
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => return,
        Some(Value::String(s)) => s.as_str(),
        Some(other) => {
            errors.push(format!(
                "Scene thumbnail '{other}' must be a relative path to a file included in the deployment."
            ));
            return;
        }
    };
    if !is_relative_thumbnail_path(thumb) {
        errors.push(format!(
            "Scene thumbnail '{thumb}' must be a relative path to a file included in the deployment."
        ));
        return;
    }
    let file_present = matches!(
        entity.get("content"),
        Some(Value::Array(items)) if items
            .iter()
            .any(|item| item.get("file").and_then(|f| f.as_str()) == Some(thumb))
    );
    if !file_present {
        errors.push(format!(
            "Scene thumbnail '{thumb}' must be a file included in the deployment."
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canon_pointer_set_normalizes_and_sorts() {
        let a = canon_pointer_set(&[json!("1,2"), json!("0,0"), json!(" 0,0 ")]);
        assert_eq!(a, vec!["0,0".to_string(), "1,2".to_string()]);
    }

    #[test]
    fn canon_pointer_numerically_normalizes() {
        assert_eq!(canon_pointer("00,00"), "0,0");
        assert_eq!(canon_pointer("-0,-0"), "0,0");
        assert_eq!(canon_pointer(" 01 , 002 "), "1,2");
        assert_eq!(canon_pointer("-05,10"), "-5,10");
        assert_eq!(canon_pointer("00,00"), canon_pointer("0,0"));
        assert_eq!(canon_pointer("not-a-parcel"), "not-a-parcel");
        assert_eq!(canon_pointer("1,2,3"), "1,2,3");
        assert_eq!(canon_pointer("1e2,3"), "1e2,3");
    }

    #[test]
    fn canon_pointer_set_treats_leading_zeros_as_equal() {
        let pointers = canon_pointer_set(&[json!("00,00"), json!("01,00")]);
        let parcels = canon_pointer_set(&[json!("0,0"), json!("1,0")]);
        assert_eq!(pointers, parcels);
    }

    #[test]
    fn parcel_bounds_validation_matches_upstream() {
        assert!(validate_parcel_in_bounds("0,0").is_ok());
        assert!(validate_parcel_in_bounds("-150,150").is_ok());
        assert!(validate_parcel_in_bounds("150,-150").is_ok());
        assert!(validate_parcel_in_bounds("151,0")
            .unwrap_err()
            .contains("Coordinate X value 151 is out of bounds"));
        assert!(validate_parcel_in_bounds("0,-151")
            .unwrap_err()
            .contains("Coordinate Y value -151 is out of bounds"));
        assert!(validate_parcel_in_bounds("garbage")
            .unwrap_err()
            .contains("Invalid coordinate format"));
    }

    #[test]
    fn is_canonical_parcel_set_rejects_noncanonical_and_duplicates() {
        assert!(is_canonical_parcel_set(&[json!("0,0"), json!("0,1")]));
        assert!(is_canonical_parcel_set(&[json!("-5,10")]));
        // empty
        assert!(!is_canonical_parcel_set(&[]));
        // duplicate literal
        assert!(!is_canonical_parcel_set(&[json!("0,0"), json!("0,0")]));
        // non-canonical spelling (leading zeros / padding / sign)
        assert!(!is_canonical_parcel_set(&[json!("00,0")]));
        assert!(!is_canonical_parcel_set(&[json!(" 0,1 ")]));
        assert!(!is_canonical_parcel_set(&[json!("-0,0")]));
        // non-string entries
        assert!(!is_canonical_parcel_set(&[json!(0)]));
    }

    #[test]
    fn pointers_equal_scene_parcels_after_canonicalization() {
        let pointers = canon_pointer_set(&[json!("0,0"), json!(" 1,1 "), json!("0,0")]);
        let parcels = canon_pointer_set(&[json!("1,1"), json!("0,0")]);
        assert_eq!(pointers, parcels);

        let mismatch = canon_pointer_set(&[json!("0,0"), json!("2,2")]);
        assert_ne!(pointers, mismatch);
    }

    #[test]
    fn extract_auth_chain_indexed_and_json() {
        let mut f = BTreeMap::new();
        f.insert("authChain[0][type]".into(), "SIGNER".into());
        f.insert("authChain[0][payload]".into(), "0xabc".into());
        f.insert("authChain[0][signature]".into(), "".into());
        let v = extract_auth_chain_from_fields(&f).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["type"], "SIGNER");

        let mut g = BTreeMap::new();
        g.insert(
            "authChain".into(),
            r#"[{"type":"SIGNER","payload":"0x1"}]"#.into(),
        );
        let v = extract_auth_chain_from_fields(&g).unwrap();
        assert_eq!(v[0]["payload"], "0x1");
    }

    #[test]
    fn extract_auth_chain_requires_something() {
        let f = BTreeMap::new();
        assert!(extract_auth_chain_from_fields(&f).is_err());
    }

    #[test]
    fn thumbnail_relative_paths_are_accepted() {
        assert!(is_relative_thumbnail_path("thumb.png"));
        assert!(is_relative_thumbnail_path("images/thumbnail.png"));
        assert!(is_relative_thumbnail_path("dir/na:me.png"));
    }

    #[test]
    fn thumbnail_non_relative_paths_are_rejected() {
        for value in [
            "https://example.com/image.png",
            "https://example.com/x\"><script>alert(1)</script><meta name=\"y",
            "//evil.example/x.png",
            "/thumb.png",
            "data:text/html,<b>x</b>",
            "javascript:alert(1)",
            "HtTpS://evil.example/x.png",
            " thumb.png",
            "thumb.png ",
            "thumb\nname.png",
            "thumb\".png",
            "thumb<img>.png",
            "\u{FEFF}thumb.png",
            "thumb.png\u{FEFF}",
        ] {
            assert!(
                !is_relative_thumbnail_path(value),
                "expected rejection: {value:?}"
            );
        }
    }

    #[test]
    fn thumbnail_validation_pushes_upstream_error_strings() {
        let mut errors = Vec::new();
        let entity = json!({
            "content": [{ "file": "https://example.com/image.png", "hash": "bafyx" }],
            "metadata": { "display": { "navmapThumbnail": "https://example.com/image.png" } }
        });
        validate_navmap_thumbnail(&entity, &mut errors);
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail 'https://example.com/image.png' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        let entity = json!({
            "content": [{ "file": "other.png", "hash": "bafyx" }],
            "metadata": { "display": { "navmapThumbnail": "thumb.png" } }
        });
        validate_navmap_thumbnail(&entity, &mut errors);
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail 'thumb.png' must be a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        let entity = json!({
            "content": [{ "file": "images/thumbnail.png", "hash": "bafyx" }],
            "metadata": { "display": { "navmapThumbnail": "images/thumbnail.png" } }
        });
        validate_navmap_thumbnail(&entity, &mut errors);
        assert!(errors.is_empty());

        let mut errors = Vec::new();
        validate_navmap_thumbnail(&json!({ "metadata": {} }), &mut errors);
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": null } } }),
            &mut errors,
        );
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": "" } } }),
            &mut errors,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn thumbnail_bom_prefixed_path_is_rejected_even_when_content_matches() {
        let mut errors = Vec::new();
        let entity = json!({
            "content": [{ "file": "\u{FEFF}thumb.png", "hash": "bafyx" }],
            "metadata": { "display": { "navmapThumbnail": "\u{FEFF}thumb.png" } }
        });
        validate_navmap_thumbnail(&entity, &mut errors);
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail '\u{FEFF}thumb.png' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );
    }

    #[test]
    fn thumbnail_non_string_values_fail_validation() {
        let mut errors = Vec::new();
        let entity = json!({
            "content": [{ "file": "thumb.png", "hash": "bafyx" }],
            "metadata": { "display": { "navmapThumbnail":
                ["https://x\"><script>alert(1)</script><meta name=\"y"] } }
        });
        validate_navmap_thumbnail(&entity, &mut errors);
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail '[\"https://x\\\"><script>alert(1)</script><meta name=\\\"y\"]' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": 5 } } }),
            &mut errors,
        );
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail '5' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": {"a": 1} } } }),
            &mut errors,
        );
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail '{\"a\":1}' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": true } } }),
            &mut errors,
        );
        assert_eq!(
            errors,
            vec![
                "Scene thumbnail 'true' must be a relative path to a file included in the deployment."
                    .to_string()
            ]
        );

        let mut errors = Vec::new();
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": false } } }),
            &mut errors,
        );
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": 0 } } }),
            &mut errors,
        );
        validate_navmap_thumbnail(
            &json!({ "metadata": { "display": { "navmapThumbnail": 0.0 } } }),
            &mut errors,
        );
        assert!(errors.is_empty(), "falsy values must skip: {errors:?}");
    }

    #[test]
    fn entity_file_cap_matches_upstream_constant_and_message() {
        assert_eq!(MAX_ENTITY_FILE_SIZE_BYTES, 5 * 1024 * 1024);
        assert_eq!(
            entity_file_too_large_error(),
            "The entity file is too large. The maximum allowed size is 5242880 bytes."
        );
    }
}
