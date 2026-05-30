//! Field validation for community name/description.
//!
//! Parity with upstream `social-service-ea`
//! (`src/logic/community/fields-validator.ts`): name must be non-empty and
//! ≤ 30 chars; description non-empty and ≤ 500 chars. Limits are measured in
//! Unicode scalar values (`chars().count()`), matching JS `String.length`'s
//! intent of "user-visible length" closely enough for a guard rail and, more
//! importantly, bounding storage.
//!
//! We additionally reject NUL and other C0/C1 control characters (except the
//! common whitespace `\t \n \r`): Postgres `TEXT` columns cannot store a NUL
//! byte, so an un-screened control char surfaces as a 500 "database error"
//! instead of a clean 400 (see /tmp/content-hostile-strings.py).

/// Max community name length (Unicode scalar values). Matches upstream's 30.
pub const NAME_MAX: usize = 30;
/// Max community description length. Matches upstream's 500.
pub const DESCRIPTION_MAX: usize = 500;

/// Reject NUL and control characters that break TEXT storage / are never
/// meaningful in a community name or description. Ordinary whitespace
/// (`\t`, `\n`, `\r`) is allowed; everything else in the C0/C1 control range
/// (including the NUL byte and the RTL-override is *not* a control char, so it
/// is intentionally allowed through — it round-trips safely as JSON data).
fn has_forbidden_control(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
}

/// Validate a community name. Returns a human-readable error string on failure.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    let len = name.chars().count();
    if len > NAME_MAX {
        return Err(format!("name must be at most {NAME_MAX} characters"));
    }
    if has_forbidden_control(name) {
        return Err("name contains forbidden control characters".to_string());
    }
    Ok(())
}

/// Validate a community description.
pub fn validate_description(description: &str) -> Result<(), String> {
    if description.trim().is_empty() {
        return Err("description is required".to_string());
    }
    let len = description.chars().count();
    if len > DESCRIPTION_MAX {
        return Err(format!(
            "description must be at most {DESCRIPTION_MAX} characters"
        ));
    }
    if has_forbidden_control(description) {
        return Err("description contains forbidden control characters".to_string());
    }
    Ok(())
}

/// Validate an *optional* name (federation update / partial edit): `None`
/// means "leave unchanged" and is always allowed.
pub fn validate_name_opt(name: Option<&str>) -> Result<(), String> {
    match name {
        Some(n) => validate_name(n),
        None => Ok(()),
    }
}

/// Validate an *optional* description.
pub fn validate_description_opt(description: Option<&str>) -> Result<(), String> {
    match description {
        Some(d) => validate_description(d),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary() {
        assert!(validate_name("My Cool Community").is_ok());
        assert!(validate_description("A nice place to hang out.").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_name("   ").is_err());
        assert!(validate_description("").is_err());
    }

    #[test]
    fn enforces_length() {
        assert!(validate_name(&"N".repeat(NAME_MAX)).is_ok());
        assert!(validate_name(&"N".repeat(NAME_MAX + 1)).is_err());
        assert!(validate_description(&"D".repeat(DESCRIPTION_MAX)).is_ok());
        assert!(validate_description(&"D".repeat(DESCRIPTION_MAX + 1)).is_err());
    }

    #[test]
    fn length_is_unicode_scalar_count_not_bytes() {
        // 30 multi-byte chars = 30 scalar values, ok; 31 rejected.
        let n30 = "é".repeat(NAME_MAX);
        assert!(validate_name(&n30).is_ok());
        let n31 = "é".repeat(NAME_MAX + 1);
        assert!(validate_name(&n31).is_err());
    }

    #[test]
    fn rejects_nul_and_controls() {
        assert!(validate_name("ab\u{0}cd").is_err()); // NUL -> would be a 500 in PG
        assert!(validate_name("a\u{7}b").is_err()); // BEL
        assert!(validate_name("a\u{1b}[31mb").is_err()); // ESC
    }

    #[test]
    fn allows_common_whitespace_and_non_control_unicode() {
        assert!(validate_description("line one\nline two\twith tab").is_ok());
        // RTL override is NOT a control char; it round-trips as JSON data.
        assert!(validate_name("abc\u{202e}xyz").is_ok());
        // homoglyphs are fine (just data)
        assert!(validate_name("аррӏе").is_ok());
    }

    #[test]
    fn opt_none_is_ok() {
        assert!(validate_name_opt(None).is_ok());
        assert!(validate_description_opt(None).is_ok());
        assert!(validate_name_opt(Some(&"N".repeat(NAME_MAX + 1))).is_err());
    }
}
