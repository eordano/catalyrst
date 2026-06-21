use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth_chain::require_signer;
use crate::http::errors::ApiError;
use crate::http::response::ApiData;
use crate::ports::items::ItemQuery;
use crate::AppState;

const CURATION_STATUSES: [&str; 3] = ["pending", "approved", "rejected"];

pub async fn get_collection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiData<Value>>, ApiError> {
    let path = format!("/v1/collections/{}", id);
    let signer = require_signer(&headers, "get", &path)?.to_ascii_lowercase();

    let collection_id = Uuid::parse_str(id.trim()).map_err(|_| {
        ApiError::not_found_with("Not found", json!({ "id": id, "eth_address": signer }))
    })?;

    let meta = state
        .items
        .collection_by_id(&collection_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found_with("Not found", json!({ "id": id, "eth_address": signer }))
        })?;

    let is_admin = state.admin_addresses.iter().any(|a| a == &signer);
    if signer != meta.eth_address.to_ascii_lowercase() && !is_admin {
        return Err(ApiError::unauthorized_with(
            "Unauthorized",
            json!({ "eth_address": signer }),
        ));
    }

    Ok(Json(ApiData::ok(meta.to_meta_json())))
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
) -> Result<Json<ApiData<Value>>, ApiError> {
    let path = format!("/v1/collections/{}/items", id);
    let signer = require_signer(&headers, "get", &path)?.to_ascii_lowercase();

    if let Some(status) = &params.status {
        if !CURATION_STATUSES.contains(&status.as_str()) {
            return Err(ApiError::bad_request_with(
                "Invalid Status provided",
                json!({ "id": id, "status": status }),
            ));
        }
    }

    let collection_id = Uuid::parse_str(id.trim()).map_err(|_| {
        ApiError::not_found_with("Not found", json!({ "id": id, "eth_address": signer }))
    })?;

    let owner = state
        .items
        .collection_owner(&collection_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found_with("Not found", json!({ "id": id, "eth_address": signer }))
        })?;

    let is_admin = state.admin_addresses.iter().any(|a| a == &signer);
    if signer != owner && !is_admin {
        return Err(ApiError::unauthorized_with(
            "Unauthorized",
            json!({ "eth_address": signer }),
        ));
    }

    let paginate = params.page.is_some() && params.limit.is_some();

    let q = ItemQuery {
        status: params.status,
        mapping_status: params.mapping_status,
        synced: params.synced,
        name: params.name,
        page: params.page,
        limit: params.limit,
    };

    let (items, total) = state.items.items_for_collection(&collection_id, &q).await?;
    let results: Vec<Value> = items.iter().map(|i| i.to_full_item()).collect();

    let data = if paginate {
        let limit = params.limit.unwrap_or(0);
        let page = params.page.unwrap_or(0);
        let pages = if limit > 0 {
            (total + limit - 1) / limit
        } else {
            0
        };
        json!({
            "total": total,
            "limit": limit,
            "pages": pages,
            "page": page,
            "results": results,
        })
    } else {
        Value::Array(results)
    };

    Ok(Json(ApiData::ok(data)))
}
