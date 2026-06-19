//! Field validation for community name/description, matching upstream
//! `social-service-ea`: name 1..=30, description 1..=500, measured in Unicode
//! scalar values.

pub const NAME_MAX: usize = 30;
pub const DESCRIPTION_MAX: usize = 500;

/// Reject NUL and other C0/C1 control chars (except `\t \n \r`): Postgres `TEXT`
/// cannot store a NUL byte, so an un-screened control char surfaces as a 500
/// instead of a clean 400.
fn has_forbidden_control(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
}

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

/// `None` means "leave unchanged" and is always allowed.
pub fn validate_name_opt(name: Option<&str>) -> Result<(), String> {
    match name {
        Some(n) => validate_name(n),
        None => Ok(()),
    }
}

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
