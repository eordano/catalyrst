//! Handlers under `/v1/users/:address/...`.
//!
//! All three sub-handlers (wearables, emotes, names) share the same paginated
//! response shape produced by `controllers/handlers/utils.ts:createPaginatedResponse`,
//! defined here in `PaginatedAssetsBody`/`AssetsHttpResponse`.

pub mod emotes;
pub mod names;
pub mod wearables;

use serde::Serialize;

/// Wrapper that mirrors `HTTPResponse<T>` from `types.ts` — `{ ok: true, data: T }`.
/// The user-asset endpoints use this shape rather than `{ data, total }`.
#[derive(Debug, Serialize)]
pub struct AssetsHttpResponse<T> {
    pub ok: bool,
    pub data: PaginatedAssetsBody<T>,
}

/// Body produced by `createPaginatedResponse(elements, total, first, skip, totalItems?)`.
#[derive(Debug, Serialize)]
pub struct PaginatedAssetsBody<T> {
    pub elements: Vec<T>,
    pub page: i64,
    pub pages: i64,
    pub limit: i64,
    pub total: i64,
    #[serde(rename = "totalItems", skip_serializing_if = "Option::is_none")]
    pub total_items: Option<i64>,
}

/// `createPaginatedResponse(elements, total, first, skip, totalItems?)`. Note
/// the special-case `limit = first || 1` and `page = floor(skip / limit) + 1`
/// (1-indexed!) that differs from `PaginatedResponse::new` in `http::response`.
pub fn create_paginated_response<T>(
    elements: Vec<T>,
    total: i64,
    first: i64,
    skip: i64,
    total_items: Option<i64>,
) -> AssetsHttpResponse<T> {
    let limit = if first == 0 { 1 } else { first };
    let page = skip / limit + 1;
    let pages = if limit > 0 {
        (total + limit - 1) / limit
    } else {
        0
    };
    AssetsHttpResponse {
        ok: true,
        data: PaginatedAssetsBody {
            elements,
            page,
            pages,
            limit,
            total,
            total_items,
        },
    }
}
