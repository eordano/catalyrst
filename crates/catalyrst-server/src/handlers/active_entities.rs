use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::AppResult;
use crate::handlers::get_entities::entities_cache_control;
use crate::state::{retain_non_denylisted, AppState};
use crate::validation::{validate_ids_or_pointers, MAX_IDS_OR_POINTERS};

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
    let use_ids = validate_ids_or_pointers(
        body.ids.as_deref(),
        body.pointers.as_deref(),
        MAX_IDS_OR_POINTERS,
    )?;
    let values = if use_ids {
        body.ids
            .as_ref()
            .expect("validate_ids_or_pointers guarantees ids is present")
    } else {
        body.pointers
            .as_ref()
            .expect("validate_ids_or_pointers guarantees pointers is present")
    };

    let mut entities: Vec<Value> = if use_ids {
        state.database.active_entities_by_ids(values).await?
    } else {
        state.database.active_entities_by_pointers(values).await?
    };

    retain_non_denylisted(&mut entities, state.denylist.as_ref());

    let mut response = Json(entities).into_response();
    if let Some(cache_control) = entities_cache_control(state.entities_cache_control_max_age) {
        if let Ok(hv) = cache_control.parse() {
            response.headers_mut().insert("Cache-Control", hv);
        }
    }
    Ok(response)
}
