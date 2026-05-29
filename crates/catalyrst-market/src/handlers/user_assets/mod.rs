pub mod emotes;
pub mod names;
pub mod wearables;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AssetsHttpResponse<T> {
    pub ok: bool,
    pub data: PaginatedAssetsBody<T>,
}

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
