use axum::extract::{Query, State};
use axum::Json;

use crate::http::errors::{ApiError, ApiResult};
use crate::resolve::{resolve_db_entities, resolve_versions};
use crate::types::{DbEntity, EntityVersions, PointersBody, WorldNameQuery, MAX_POINTERS};
use crate::AppState;

pub async fn post_entities_active(
    State(state): State<AppState>,
    Query(q): Query<WorldNameQuery>,
    Json(body): Json<PointersBody>,
) -> ApiResult<Json<Vec<DbEntity>>> {
    let pointers = validate_pointers(body.pointers)?;
    let entities = resolve_db_entities(&state, &pointers, q.world_name.as_deref()).await?;
    Ok(Json(entities))
}

pub async fn post_entities_versions(
    State(state): State<AppState>,
    Query(q): Query<WorldNameQuery>,
    Json(body): Json<PointersBody>,
) -> ApiResult<Json<Vec<EntityVersions>>> {
    let pointers = validate_pointers(body.pointers)?;
    let versions = resolve_versions(&state, &pointers, q.world_name.as_deref()).await?;
    Ok(Json(versions))
}

fn validate_pointers(pointers: Vec<String>) -> Result<Vec<String>, ApiError> {
    if pointers.is_empty() {
        return Err(ApiError::bad_request("pointers must be a non-empty array"));
    }
    if pointers.len() > MAX_POINTERS {
        return Err(ApiError::bad_request(format!(
            "too many pointers: {} (max {})",
            pointers.len(),
            MAX_POINTERS
        )));
    }
    Ok(pointers)
}
