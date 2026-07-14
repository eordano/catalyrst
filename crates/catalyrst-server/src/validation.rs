use crate::errors::{AppError, InvalidRequestError};

pub const MAX_IDS_OR_POINTERS: usize = 1000;

pub fn validate_ids_or_pointers(
    ids: Option<&[String]>,
    pointers: Option<&[String]>,
    max: usize,
) -> Result<bool, AppError> {
    let is_valid = |s: &String| !s.is_empty() && !s.contains('\0');
    let (len, use_ids) = match (ids, pointers) {
        (Some(ids), None) if !ids.is_empty() && ids.iter().all(is_valid) => (ids.len(), true),
        (None, Some(pointers)) if !pointers.is_empty() && pointers.iter().all(is_valid) => {
            (pointers.len(), false)
        }
        _ => {
            return Err(InvalidRequestError::new(
                "ids or pointers must be present, but not both. \
                 They must be arrays and contain at least one element. \
                 None of the elements can be empty or contain NUL bytes.",
            )
            .into());
        }
    };

    if len > max {
        return Err(InvalidRequestError::new(format!(
            "Too many ids or pointers; the maximum allowed is {}",
            max
        ))
        .into());
    }

    Ok(use_ids)
}

pub fn is_valid_content_hash(hash: &str) -> bool {
    if hash.starts_with("Qm")
        && hash.len() == 46
        && hash[2..].chars().all(|c| c.is_ascii_alphanumeric())
    {
        return true;
    }

    if hash.starts_with("ba")
        && hash.len() >= 52
        && hash
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return true;
    }

    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_cidv0() {
        assert!(is_valid_content_hash(
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"
        ));
    }

    #[test]
    fn valid_cidv1() {
        assert!(is_valid_content_hash(
            "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenora7777"
        ));
    }

    #[test]
    fn valid_legacy_sha256() {
        assert!(is_valid_content_hash(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
        ));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_content_hash("../../../etc/passwd"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_valid_content_hash(""));
    }

    #[test]
    fn rejects_arbitrary_string() {
        assert!(!is_valid_content_hash("hello world"));
    }

    #[test]
    fn rejects_too_short_cidv0() {
        assert!(!is_valid_content_hash("QmTooShort"));
    }

    #[test]
    fn rejects_cidv1_with_uppercase() {
        assert!(!is_valid_content_hash(
            "bafkreihdwdcefgh4dqkjv67uzcmw7oJEE6xedzdetojuzjevtenora7777"
        ));
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn validate(ids: Option<&[String]>, pointers: Option<&[String]>) -> Result<bool, AppError> {
        validate_ids_or_pointers(ids, pointers, MAX_IDS_OR_POINTERS)
    }

    #[test]
    fn accepts_ids_only() {
        let ids = strings(&["a", "b"]);
        assert!(validate(Some(&ids), None).unwrap());
    }

    #[test]
    fn accepts_pointers_only() {
        let ptrs = strings(&["0,0"]);
        assert!(!validate(None, Some(&ptrs)).unwrap());
    }

    #[test]
    fn rejects_both_present() {
        let ids = strings(&["a"]);
        let ptrs = strings(&["0,0"]);
        let err = validate(Some(&ids), Some(&ptrs)).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("but not both"));
    }

    #[test]
    fn rejects_neither_present() {
        let err = validate(None, None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("at least one element"));
    }

    #[test]
    fn rejects_empty_ids_array() {
        let empty: Vec<String> = Vec::new();
        let err = validate(Some(&empty), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("at least one element"));
    }

    #[test]
    fn rejects_empty_string_element_in_ids() {
        let ids = strings(&["a", "", "c"]);
        let err = validate(Some(&ids), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err
            .to_string()
            .contains("None of the elements can be empty"));
    }

    #[test]
    fn rejects_empty_string_element_in_pointers() {
        let ptrs = strings(&[""]);
        let err = validate(None, Some(&ptrs)).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err
            .to_string()
            .contains("None of the elements can be empty"));
    }

    #[test]
    fn rejects_nul_byte_element_in_ids() {
        let ids = strings(&["a", "\0", "c"]);
        let err = validate(Some(&ids), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("NUL bytes"));
    }

    #[test]
    fn rejects_nul_byte_element_in_pointers() {
        let ptrs = strings(&["0,0\0"]);
        let err = validate(None, Some(&ptrs)).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("NUL bytes"));
    }

    #[test]
    fn accepts_exactly_1000_ids() {
        let ids: Vec<String> = (0..MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(validate(Some(&ids), None).unwrap());
    }

    #[test]
    fn rejects_over_1000_ids() {
        let ids: Vec<String> = (0..=MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), 1001);
        let err = validate(Some(&ids), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert_eq!(
            err.to_string(),
            "Too many ids or pointers; the maximum allowed is 1000"
        );
    }

    #[test]
    fn rejects_over_1000_pointers() {
        let ptrs: Vec<String> = (0..=MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        let err = validate(None, Some(&ptrs)).unwrap_err();
        assert!(err.to_string().contains("Too many ids or pointers"));
    }
}
