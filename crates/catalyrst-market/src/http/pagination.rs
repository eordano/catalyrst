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
