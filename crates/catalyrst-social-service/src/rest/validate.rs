use icu_properties::props::DefaultIgnorableCodePoint;
use icu_properties::CodePointSetData;
use unicode_normalization::UnicodeNormalization;

pub const NAME_MAX: usize = 30;
pub const DESCRIPTION_MAX: usize = 500;

fn has_forbidden_control(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
}

/// A single lowercase code point for confusable lookup, or None when the char folds to a sequence
/// (no such multi-char key exists in the table, so a sequence can never match one).
fn single_lowercase(c: char) -> Option<char> {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => Some(l),
        _ => None,
    }
}

/// Letters from other scripts whose common rendering is indistinguishable from a Latin one, mapped
/// to what they imitate. Deliberately narrow: only lookalikes a reader could not tell apart, so a
/// name merely written in another script stays distinct (port of #483).
fn confusable_to_latin(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{430}' => "a",
        '\u{432}' => "b",
        '\u{435}' => "e",
        '\u{455}' => "s",
        '\u{456}' => "i",
        '\u{458}' => "j",
        '\u{43A}' => "k",
        '\u{43C}' => "m",
        '\u{43D}' => "h",
        '\u{43E}' => "o",
        '\u{440}' => "p",
        '\u{441}' => "c",
        '\u{442}' => "t",
        '\u{443}' => "y",
        '\u{445}' => "x",
        '\u{501}' => "d",
        '\u{51B}' => "q",
        '\u{51D}' => "w",
        '\u{461}' => "w",
        '\u{493}' => "f",
        '\u{4CF}' => "l",
        '\u{4C0}' => "l",
        '\u{4D5}' => "ae",
        '\u{3B1}' => "a",
        '\u{3B2}' => "b",
        '\u{3B5}' => "e",
        '\u{3B9}' => "i",
        '\u{3BA}' => "k",
        '\u{3BD}' => "v",
        '\u{3BF}' => "o",
        '\u{3C1}' => "p",
        '\u{3C4}' => "t",
        '\u{3C5}' => "u",
        '\u{3C7}' => "x",
        '\u{3B3}' => "y",
        '\u{3F2}' => "c",
        '\u{3F3}' => "j",
        '\u{A7B7}' => "w",
        '\u{475}' => "v",
        '\u{131}' => "i",
        '\u{237}' => "j",
        '\u{261}' => "g",
        '\u{269}' => "i",
        '\u{1D0F}' => "o",
        '\u{1D20}' => "v",
        _ => return None,
    })
}

/// Reduces a name to the form used for restricted-name comparison and the emptiness check.
///
/// Raw code-point equality treats a reserved name and the same name wearing one invisible
/// character, a fullwidth variant, a decomposed accent, or a cross-script lookalike as different
/// strings while a reader sees one name. The fold removes those differences before comparing:
/// NFKC (collapses compatibility forms, recomposes decomposed marks), removal of characters that
/// occupy no visual space wherever they appear, mapping of cross-script letters that render as
/// Latin, then lowercasing and whitespace collapse. Used only for comparison; the submitted name
/// is stored as given (port of #483).
pub fn fold_name_for_comparison(name: &str) -> String {
    let ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
    let mut mapped = String::with_capacity(name.len());
    for c in name.nfkc() {
        // U+2800 BRAILLE PATTERN BLANK renders as nothing but is a symbol, so no property claims it.
        if ignorable.contains(c) || c == '\u{2800}' {
            continue;
        }
        match single_lowercase(c).and_then(confusable_to_latin) {
            Some(s) => mapped.push_str(s),
            None => mapped.push(c),
        }
    }
    let mut out = String::with_capacity(mapped.len());
    let mut pending_space = false;
    for c in mapped.to_lowercase().chars() {
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
    }
    out
}

pub fn validate_name(name: &str) -> Result<(), String> {
    // An all-invisible name folds to empty, so the emptiness check reads the same reduced form the
    // restricted list is compared against rather than raw `trim`, which strips only whitespace.
    if fold_name_for_comparison(name).is_empty() {
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

/// Rejects a name that folds to a configured restricted entry. The entries are already folded at
/// config load, so both sides are reduced the same way. A no-op until RESTRICTED_NAMES is populated
/// (port of #483).
pub fn check_restricted_name(name: &str, restricted: &[String]) -> Result<(), String> {
    if restricted.is_empty() {
        return Ok(());
    }
    let folded = fold_name_for_comparison(name);
    if restricted.contains(&folded) {
        return Err("name is not allowed".to_string());
    }
    Ok(())
}

pub fn check_restricted_name_opt(name: Option<&str>, restricted: &[String]) -> Result<(), String> {
    match name {
        Some(n) => check_restricted_name(n, restricted),
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
    fn rejects_all_invisible_name() {
        assert!(validate_name("\u{200B}\u{2060}").is_err());
        assert!(validate_name("\u{2800}\u{2800}").is_err());
        assert!(validate_name("\u{E0041}").is_err());
        assert!(validate_name("\u{1D173}").is_err());
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
        let n30 = "\u{E9}".repeat(NAME_MAX);
        assert!(validate_name(&n30).is_ok());
        let n31 = "\u{E9}".repeat(NAME_MAX + 1);
        assert!(validate_name(&n31).is_err());
    }

    #[test]
    fn rejects_nul_and_controls() {
        assert!(validate_name("ab\u{0}cd").is_err());
        assert!(validate_name("a\u{7}b").is_err());
        assert!(validate_name("a\u{1b}[31mb").is_err());
    }

    #[test]
    fn allows_common_whitespace_and_non_control_unicode() {
        assert!(validate_description("line one\nline two\twith tab").is_ok());

        assert!(validate_name("abc\u{202e}xyz").is_ok());

        assert!(validate_name("\u{430}\u{440}\u{440}\u{4CF}\u{435}").is_ok());
    }

    #[test]
    fn opt_none_is_ok() {
        assert!(validate_name_opt(None).is_ok());
        assert!(validate_description_opt(None).is_ok());
        assert!(validate_name_opt(Some(&"N".repeat(NAME_MAX + 1))).is_err());
    }

    #[test]
    fn fold_strips_invisibles_wherever_they_appear() {
        assert_eq!(fold_name_for_comparison("admin\u{200B}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{2800}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{00AD}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{3164}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{FE0F}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{180F}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{E0100}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{E007F}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{1BCA0}"), "admin");
        assert_eq!(fold_name_for_comparison("admin\u{1D173}"), "admin");
        assert_eq!(fold_name_for_comparison("ad\u{200B}min"), "admin");
    }

    #[test]
    fn fold_normalizes_lookalike_forms() {
        assert_eq!(fold_name_for_comparison("\u{0430}dmin"), "admin");
        assert_eq!(
            fold_name_for_comparison("\u{FF41}\u{FF44}\u{FF4D}\u{FF49}\u{FF4E}"),
            "admin"
        );
        assert_eq!(
            fold_name_for_comparison("decentra\u{04CF}and"),
            "decentraland"
        );
        assert_eq!(
            fold_name_for_comparison("decentra\u{04C0}and"),
            "decentraland"
        );
    }

    #[test]
    fn restricted_rejects_folded_matches() {
        let restricted = vec!["admin".to_string()];
        assert!(check_restricted_name("admin", &restricted).is_err());
        assert!(check_restricted_name("ADMIN", &restricted).is_err());
        assert!(check_restricted_name("admin\u{200B}", &restricted).is_err());
        assert!(check_restricted_name("ad\u{200B}min", &restricted).is_err());
        assert!(check_restricted_name("\u{0430}dmin", &restricted).is_err());
    }

    #[test]
    fn restricted_allows_near_misses_and_other_scripts() {
        let restricted = vec!["admin".to_string()];
        assert!(check_restricted_name("administrator", &restricted).is_ok());
        assert!(check_restricted_name("admin fans", &restricted).is_ok());
        assert!(check_restricted_name("\u{0434}\u{043E}\u{043C}", &restricted).is_ok());
    }

    #[test]
    fn restricted_palochka_matches_decentraland() {
        let restricted = vec![fold_name_for_comparison("decentraland")];
        assert!(check_restricted_name("decentra\u{04CF}and", &restricted).is_err());
        assert!(check_restricted_name("decentra\u{04C0}and", &restricted).is_err());
    }

    #[test]
    fn restricted_empty_list_is_noop() {
        assert!(check_restricted_name("admin", &[]).is_ok());
        assert!(check_restricted_name_opt(None, &["admin".to_string()]).is_ok());
    }
}
