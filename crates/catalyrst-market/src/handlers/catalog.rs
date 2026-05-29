//! Direct port of `marketplace-server/src/controllers/handlers/catalog-handler.ts`.
//!
//! `/v1/catalog` and `/v2/catalog` both route here — the V2 path turns on the
//! `mv_trades` (offchain orders) joins via the `is_v2` flag.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::http::response::{ApiError, DataTotal};
use crate::logic::catalog::parse_catalog_filters;
use crate::ports::catalog::CatalogItem;
use crate::AppState;

/// `GET /v1/catalog`.
pub async fn get_catalog_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<DataTotal<CatalogItem>>, ApiError> {
    get_catalog_inner(state, headers, pairs, false).await
}

/// `GET /v2/catalog`.
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

    // Headers — picked off but no-op for now (analytics is out of scope).
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

    let (data, total) = state.catalog.fetch(filters, &search_id, &anon_id, is_v2).await?;
    Ok(Json(DataTotal { data, total }))
}
