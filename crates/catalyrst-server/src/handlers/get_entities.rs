use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::formatters::{mask_entity, EntityField};
use crate::query_params::{parse_query_string, qs_get_array, qs_get_string};
use crate::state::AppState;

pub async fn get_entities(
    State(state): State<Arc<AppState>>,
    Path(entity_type): Path<String>,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let query_string = request.uri().query().unwrap_or("");
    let params = parse_query_string(query_string);

    let normalized = {
        let mut s = entity_type.trim().to_lowercase();
        if s.ends_with('s') {
            s.pop();
        }
        s
    };

    let valid_types = ["scene", "profile", "wearable", "store", "emote"];
    if !valid_types.contains(&normalized.as_str()) {
        return Err(InvalidRequestError::new(format!("Unrecognized type: {}", entity_type)).into());
    }

    let pointers: Vec<String> = qs_get_array(&params, "pointer")
        .into_iter()
        .map(|p| p.to_lowercase())
        .collect();
    let ids = qs_get_array(&params, "id");

    if (ids.is_empty() && pointers.is_empty()) || (!ids.is_empty() && !pointers.is_empty()) {
        return Err(InvalidRequestError::new("ids or pointers must be present, but not both").into());
    }

    let fields_param = qs_get_string(&params, "fields");
    let fields: Option<Vec<EntityField>> = fields_param.map(|f| {
        f.split(',')
            .filter_map(|s| EntityField::parse(s.trim()))
            .collect()
    });

    let entities: Vec<Value> = if !ids.is_empty() {
        state
            .database
            .active_entities_by_ids(&ids)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        state
            .database
            .active_entities_by_pointers(&pointers)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    let masked: Vec<Value> = entities
        .iter()
        .map(|e| mask_entity(e, fields.as_deref()))
        .collect();

    Ok(Json(Value::Array(masked)))
}
