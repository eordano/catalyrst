pub use catalyrst_types::{
    HttpError, InvalidParameterError, MarketplaceApiError as ApiError, PageInput as Pagination,
    PaginatedResponse,
};

use axum::extract::Query;
use serde::{Deserialize, Serialize};

const MAX_LIMIT: i64 = 100;

pub fn get_pagination_params(pairs: &[(String, String)]) -> Pagination {
    let mut limit_raw: Option<&str> = None;
    let mut offset_raw: Option<&str> = None;
    for (k, v) in pairs {
        match k.as_str() {
            "limit" if limit_raw.is_none() => limit_raw = Some(v),
            "offset" if offset_raw.is_none() => offset_raw = Some(v),
            _ => {}
        }
    }
    let limit = match limit_raw.and_then(|s| s.parse::<i64>().ok()) {
        Some(n) if n > 0 && n <= MAX_LIMIT => n,
        _ => MAX_LIMIT,
    };
    let offset = match offset_raw.and_then(|s| s.parse::<i64>().ok()) {
        Some(n) if n >= 0 => n,
        _ => 0,
    };
    Pagination { limit, offset }
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

#[derive(Debug, Serialize)]
pub struct EnvelopeData<T> {
    pub data: T,
}

#[derive(Debug, Serialize)]
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
        Self { results, total, page, pages, limit }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RawQuery(pub Vec<(String, String)>);

impl RawQuery {
    pub fn from_axum(Query(pairs): Query<Vec<(String, String)>>) -> Self {
        Self(pairs)
    }
}
