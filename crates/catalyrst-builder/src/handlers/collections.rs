use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::items::{CollectionMetaOut, FullItemOut, ItemQuery};
use crate::AppState;

const CURATION_STATUSES: [&str; 3] = ["pending", "approved", "rejected"];

const DEFAULT_LIMIT: i64 = 100_000;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
pub struct PaginatedFullItemsOut {
    pub total: i64,
    pub limit: i64,
    pub pages: i64,
    pub page: i64,
    pub results: Vec<FullItemOut>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "builder/"))]
#[serde(untagged)]
pub enum CollectionItemsOut {
    Plain(Vec<FullItemOut>),
    Paginated(PaginatedFullItemsOut),
}

fn should_paginate(page: Option<i64>, limit: Option<i64>) -> bool {
    matches!((page, limit), (Some(p), Some(l)) if p != 0 && l != 0 && l < DEFAULT_LIMIT)
}

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiData<CollectionMetaOut>>, ApiError> {
    let path = format!("/v1/collections/{}", id);
    let signer = require_signer(&headers, "get", &path).await?;

    let collection_id = Uuid::parse_str(id.trim()).map_err(|_| ApiError::not_found("Not found"))?;

    let meta = state
        .items
        .collection_by_id(&collection_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Not found"))?;

    let is_admin = state.admin_addresses.iter().any(|a| a == &signer);
    if signer != meta.eth_address.to_ascii_lowercase() && !is_admin {
        return Err(ApiError::unauthorized("Unauthorized"));
    }

    Ok(Json(ApiData::ok(CollectionMetaOut::from(&meta))))
}

#[derive(Debug, Default, Deserialize)]
pub struct CollectionItemsParams {
    pub status: Option<String>,
    #[serde(rename = "mappingStatus")]
    pub mapping_status: Option<String>,
    pub synced: Option<bool>,
    pub name: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_collection_items(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<CollectionItemsParams>,
    headers: HeaderMap,
) -> Result<Json<ApiData<CollectionItemsOut>>, ApiError> {
    let path = format!("/v1/collections/{}/items", id);
    let signer = require_signer(&headers, "get", &path).await?;

    if let Some(status) = &params.status {
        if !CURATION_STATUSES.contains(&status.as_str()) {
            return Err(ApiError::bad_request("Invalid Status provided"));
        }
    }

    let collection_id = Uuid::parse_str(id.trim()).map_err(|_| ApiError::not_found("Not found"))?;

    let owner = state
        .items
        .collection_owner(&collection_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Not found"))?;

    let is_admin = state.admin_addresses.iter().any(|a| a == &signer);
    if signer != owner && !is_admin {
        return Err(ApiError::unauthorized("Unauthorized"));
    }

    let paginate = should_paginate(params.page, params.limit);

    let q = ItemQuery {
        status: params.status,
        mapping_status: params.mapping_status,
        synced: params.synced,
        name: params.name,
        page: params.page,
        limit: params.limit,
    };

    let (items, total) = state.items.items_for_collection(&collection_id, &q).await?;
    let results: Vec<FullItemOut> = items.iter().map(|i| i.to_out()).collect();

    let data = if paginate {
        let limit = params.limit.unwrap_or(0);
        let page = params.page.unwrap_or(0);
        let pages = if limit > 0 {
            (total + limit - 1) / limit
        } else {
            0
        };
        CollectionItemsOut::Paginated(PaginatedFullItemsOut {
            total,
            limit,
            pages,
            page,
            results,
        })
    } else {
        CollectionItemsOut::Plain(results)
    };

    Ok(Json(ApiData::ok(data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pagination_truthiness_matches_upstream_js() {
        assert!(should_paginate(Some(1), Some(10)));
        assert!(should_paginate(Some(1), Some(99_999)));
        assert!(!should_paginate(None, Some(10)));
        assert!(!should_paginate(Some(1), None));
        assert!(!should_paginate(None, None));
        assert!(!should_paginate(Some(0), Some(10)));
        assert!(!should_paginate(Some(1), Some(0)));
        assert!(!should_paginate(Some(1), Some(100_000)));
        assert!(!should_paginate(Some(1), Some(100_001)));
    }

    #[test]
    fn collection_items_union_arms_serialize_correctly() {
        let plain = ApiData::ok(CollectionItemsOut::Plain(vec![]));
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            json!({ "ok": true, "data": [] }),
        );

        let paginated = ApiData::ok(CollectionItemsOut::Paginated(PaginatedFullItemsOut {
            total: 5,
            limit: 2,
            pages: 3,
            page: 1,
            results: vec![],
        }));
        assert_eq!(
            serde_json::to_value(&paginated).unwrap(),
            json!({
                "ok": true,
                "data": { "total": 5, "limit": 2, "pages": 3, "page": 1, "results": [] },
            }),
        );
    }
}
