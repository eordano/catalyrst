use super::errors::InvalidParameterError;

pub use catalyrst_types::PageInput as Pagination;

const MAX_LIMIT: i64 = 100;
const DEFAULT_PAGE: i64 = 0;

pub fn get_pagination_params(pairs: &[(String, String)]) -> Pagination {
    let mut limit_raw: Option<&str> = None;
    let mut offset_raw: Option<&str> = None;
    let mut page_raw: Option<&str> = None;

    for (k, v) in pairs {
        match k.as_str() {
            "limit" if limit_raw.is_none() => limit_raw = Some(v),
            "offset" if offset_raw.is_none() => offset_raw = Some(v),
            "page" if page_raw.is_none() => page_raw = Some(v),
            _ => {}
        }
    }

    let parsed_limit = limit_raw.and_then(|s| s.parse::<i64>().ok());
    let parsed_offset = offset_raw.and_then(|s| s.parse::<i64>().ok());
    let parsed_page = page_raw.and_then(|s| s.parse::<i64>().ok());

    let limit = match parsed_limit {
        Some(n) if n > 0 && n <= MAX_LIMIT => n,
        _ => MAX_LIMIT,
    };

    let offset = match parsed_offset {
        Some(n) => n,
        None => match parsed_page {
            Some(p) if p >= 0 => p * limit,
            _ => DEFAULT_PAGE * limit,
        },
    };

    Pagination { limit, offset }
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

pub fn get_number_parameter(
    name: &str,
    pairs: &[(String, String)],
) -> Result<Option<i64>, InvalidParameterError> {
    let raw = match get_parameter(name, pairs, None)? {
        Some(v) => v,
        None => return Ok(None),
    };
    raw.parse::<i64>()
        .map(Some)
        .map_err(|_| InvalidParameterError::new(name, raw))
}
