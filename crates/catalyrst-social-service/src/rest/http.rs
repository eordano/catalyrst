pub use catalyrst_types::{
    HttpError, InvalidParameterError, MarketplaceApiError as ApiError, PageInput as Pagination,
    PaginatedResponse,
};

use serde::{Deserialize, Serialize};

const MAX_LIMIT: i64 = 100;
// Capped, not rejected, matching upstream @dcl/http-commons 2.0.1 (#485) and the RPC surface's
// MAX_PAGINATION_OFFSET: a deep offset makes Postgres walk and discard every skipped row, and
// paging past the cap returns the last page again rather than an empty result.
const MAX_OFFSET: i64 = 100_000;

pub fn get_pagination_params(pairs: &[(String, String)]) -> Pagination {
    let mut p = catalyrst_types::get_pagination_params(pairs, MAX_LIMIT);
    p.offset = p.offset.clamp(0, MAX_OFFSET);
    p
}

pub fn get_first(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

pub fn get_all(pairs: &[(String, String)], key: &str) -> Vec<String> {
    pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

pub fn get_bool(pairs: &[(String, String)], key: &str) -> bool {
    get_first(pairs, key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnvelopeData<T> {
    pub data: T,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Paginated<T> {
    pub results: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub pages: i64,
    pub limit: i64,
}

impl<T> Paginated<T> {
    pub fn new(results: Vec<T>, total: i64, pagination: &Pagination) -> Self {
        let limit = pagination.limit;
        let page = if limit > 0 {
            (pagination.offset / limit) + 1
        } else {
            1
        };
        let pages = if limit > 0 {
            (total + limit - 1) / limit
        } else {
            0
        };
        Self {
            results,
            total,
            page,
            pages,
            limit,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RawQuery(pub Vec<(String, String)>);

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(s: &[(&str, &str)]) -> Vec<(String, String)> {
        s.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn offset_is_capped_and_ordinary_values_pass_through() {
        assert_eq!(
            get_pagination_params(&pairs(&[("offset", "250")])).offset,
            250
        );
        assert_eq!(
            get_pagination_params(&pairs(&[("offset", "1000000")])).offset,
            MAX_OFFSET
        );
        assert_eq!(
            get_pagination_params(&pairs(&[("page", "999999999"), ("limit", "100")])).offset,
            MAX_OFFSET
        );
    }
}
