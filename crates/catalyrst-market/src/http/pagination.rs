use super::errors::InvalidParameterError;

pub use catalyrst_types::PageInput as Pagination;

const MAX_LIMIT: i64 = 100;

pub fn get_pagination_params(pairs: &[(String, String)]) -> Pagination {
    catalyrst_types::get_pagination_params(pairs, MAX_LIMIT)
}

pub fn get_parameter(
    name: &str,
    pairs: &[(String, String)],
    values: Option<&[&str]>,
) -> Result<Option<String>, InvalidParameterError> {
    let parameter = pairs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone());

    if let (Some(allowed), Some(ref v)) = (values, &parameter) {
        if !allowed.iter().any(|a| a == v) {
            return Err(InvalidParameterError::new(name, v.clone()));
        }
    }
    Ok(parameter)
}

/// Mirrors upstream `getNumberParameter` (logic/http/pagination.ts:34-46): an absent or empty
/// value is not an error, and the value is read with `Number.parseInt` semantics.
pub fn get_number_parameter(
    name: &str,
    pairs: &[(String, String)],
) -> Result<Option<i64>, InvalidParameterError> {
    let raw = match get_parameter(name, pairs, None)? {
        Some(v) if !v.is_empty() => v,
        _ => return Ok(None),
    };
    js_parse_int(&raw)
        .map(Some)
        .ok_or_else(|| InvalidParameterError::new(name, raw))
}

/// `Number.parseInt` without a radix: leading whitespace (JS `StrWhiteSpace` also counts
/// U+FEFF, which Rust's `char::is_whitespace` does not), an optional sign, an optional `0x`
/// prefix selecting base 16, then the leading digits of that base. A value too wide for `i64`
/// saturates rather than erroring, because JS yields a lossy float there and never throws.
fn js_parse_int(raw: &str) -> Option<i64> {
    let trimmed = raw.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let (negative, rest) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (radix, rest) = match rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        Some(rest) => (16u32, rest),
        None => (10u32, rest),
    };
    let digits: String = rest.chars().take_while(|c| c.is_digit(radix)).collect();
    if digits.is_empty() {
        return None;
    }
    Some(match i64::from_str_radix(&digits, radix) {
        Ok(value) if negative => -value,
        Ok(value) => value,
        Err(_) if negative => i64::MIN,
        Err(_) => i64::MAX,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(raw: &[(&str, &str)]) -> Vec<(String, String)> {
        raw.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn an_absent_or_empty_number_parameter_is_not_an_error() {
        assert_eq!(get_number_parameter("power", &pairs(&[])).unwrap(), None);
        assert_eq!(
            get_number_parameter("power", &pairs(&[("power", "")])).unwrap(),
            None
        );
    }

    #[test]
    fn a_number_parameter_is_read_leniently_like_parse_int() {
        for (raw, want) in [
            ("12", 12),
            ("12abc", 12),
            (" 7 ", 7),
            ("+5", 5),
            ("-3", -3),
            ("1e3", 1),
            ("08", 8),
            ("0x10", 16),
            ("\u{feff}12", 12),
            ("99999999999999999999", i64::MAX),
            ("-99999999999999999999", i64::MIN),
        ] {
            assert_eq!(
                get_number_parameter("power", &pairs(&[("power", raw)])).unwrap(),
                Some(want),
                "{raw}"
            );
        }
    }

    #[test]
    fn a_number_parameter_without_leading_digits_is_invalid() {
        for raw in ["abc", "-", "0x", "e3"] {
            let err = get_number_parameter("power", &pairs(&[("power", raw)])).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("The value of the power parameter is invalid: {raw}")
            );
        }
    }
}
