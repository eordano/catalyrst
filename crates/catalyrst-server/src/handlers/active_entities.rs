use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct ActiveEntitiesRequest {
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    #[serde(default)]
    pub pointers: Option<Vec<String>>,
}

pub async fn get_active_entities(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveEntitiesRequest>,
) -> AppResult<impl IntoResponse> {
    let entities = match (&body.ids, &body.pointers) {
        (Some(ids), None) if !ids.is_empty() => {
            state
                .database
                .active_entities_by_ids(ids)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?
        }
        (None, Some(pointers)) if !pointers.is_empty() => {
            state
                .database
                .active_entities_by_pointers(pointers)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?
        }
        _ => {
            return Err(InvalidRequestError::new(
                "ids or pointers must be present, but not both. \
                 They must be arrays and contain at least one element. \
                 None of the elements can be empty.",
            )
            .into());
        }
    };

    let filtered: Vec<Value> = entities
        .into_iter()
        .filter(|entity| {
            entity
                .get("id")
                .and_then(|id| id.as_str())
                .map(|id| !state.denylist.is_denylisted(id))
                .unwrap_or(true)
        })
        .collect();

    // Short, opt-in cache window so shared caches can absorb repeated identical reads. Default 10s,
    // tunable via ENTITIES_CACHE_CONTROL_MAX_AGE (0 disables). Active entities are mutable, so this is
    // a small staleness/perf tradeoff, not the immutable caching used for content blobs.
    let mut response = Json(filtered).into_response();
    if let Some(cache_control) =
        crate::handlers::get_entities::entities_cache_control(state.entities_cache_control_max_age)
    {
        if let Ok(hv) = cache_control.parse() {
            response.headers_mut().insert("Cache-Control", hv);
        }
    }
    Ok(response)
}
