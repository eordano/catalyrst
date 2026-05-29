//! Shared pagination input + output envelope types.
//!
//! Two types live here:
//!
//! - [`PageInput`] — the minimal `{ limit, offset }` shape parsed off a
//!   marketplace-style query string. Mirrors the input that
//!   `catalyrst-market`'s `get_pagination_params` produces, kept here so any
//!   future port (lambdas, places, events) can use the same envelope.
//! - [`PaginatedResponse<T>`] — the marketplace-server JSON output envelope
//!   (`{ results, total, page, pages, limit }`) plus a `new(results, total,
//!   limit, offset)` constructor that derives `page` and `pages`.
//!
//! Note: a richer `Pagination` struct exists in `entity.rs` (the
//! content-server flavour with `{ offset, limit, page_size, page_num }`) and
//! is intentionally left alone — these two types serve different callers.

use serde::Serialize;

/// Minimal pagination input: just `limit` + `offset`. Marketplace-style.
///
/// This mirrors the shape `catalyrst-market`'s `get_pagination_params` parses
/// out of a query string. Kept here so future ports can reuse the type.
#[derive(Debug, Clone, Copy)]
pub struct PageInput {
    pub limit: i64,
    pub offset: i64,
}

/// Marketplace-server JSON output envelope for paginated endpoints.
///
/// Shape: `{ results, total, page, pages, limit }`. Construct with
/// [`PaginatedResponse::new`] which derives `page = offset / limit` and
/// `pages = ceil(total / limit)`.
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub results: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub pages: i64,
    pub limit: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(results: Vec<T>, total: i64, limit: i64, offset: i64) -> Self {
        let page = if limit > 0 { offset / limit } else { 0 };
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
