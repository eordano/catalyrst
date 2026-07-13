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
}
