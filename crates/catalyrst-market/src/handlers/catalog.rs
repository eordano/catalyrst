use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::http::response::{ApiError, DataTotal};
use crate::logic::catalog::parse_catalog_filters;
use crate::ports::catalog::CatalogItem;
use crate::AppState;

pub async fn get_catalog_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<CatalogItem>>, ApiError> {
    get_catalog_inner(state, headers, pairs, false).await
}

pub async fn get_catalog_v2(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<CatalogItem>>, ApiError> {
    get_catalog_inner(state, headers, pairs, true).await
}

async fn get_catalog_inner(
    state: AppState,
    headers: HeaderMap,
    pairs: Vec<(String, String)>,
    is_v2: bool,
) -> Result<Json<DataTotal<CatalogItem>>, ApiError> {
    let filters = parse_catalog_filters(&pairs, is_v2)?;

    let search_id = headers
        .get("X-Search-Uuid")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let anon_id = headers
        .get("X-Anonymous-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let (data, total) = state
        .catalog
        .fetch(filters, &search_id, &anon_id, is_v2)
        .await?;
    Ok(Json(DataTotal { data, total }))
}
