use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::formatters::{mask_entity, EntityField};
use crate::query_params::{parse_query_string, qs_get_array, qs_get_string};
use crate::state::AppState;

// Bound the request-controlled `id`/`pointer` arrays that feed the `ANY(...)`/overlap query, matching
// the cap on POST /entities/active. This public, unauthenticated endpoint would otherwise let one
// request push an unbounded number of values into the query.
const MAX_IDS_OR_POINTERS: usize = 1000;

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
        return Err(
            InvalidRequestError::new("ids or pointers must be present, but not both").into(),
        );
    }

    if ids.len() > MAX_IDS_OR_POINTERS || pointers.len() > MAX_IDS_OR_POINTERS {
        return Err(InvalidRequestError::new(format!(
            "Too many ids or pointers; the maximum allowed is {}",
            MAX_IDS_OR_POINTERS
        ))
        .into());
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

    // Drop any denylisted entity, as the sibling listing endpoints (active-entities) do.
    let masked: Vec<Value> = entities
        .iter()
        .filter(|e| {
            e.get("id")
                .and_then(|id| id.as_str())
                .map(|id| !state.denylist.is_denylisted(id))
                .unwrap_or(true)
        })
        .map(|e| mask_entity(e, fields.as_deref()))
        .collect();

    // Short, opt-in cache window (default 10s, via ENTITIES_CACHE_CONTROL_MAX_AGE; 0 disables).
    // Active entities are mutable, so this is a small staleness/perf tradeoff, not the immutable
    // caching used for content blobs.
    let mut response = Json(Value::Array(masked)).into_response();
    if let Some(cache_control) = entities_cache_control(state.entities_cache_control_max_age) {
        if let Ok(hv) = cache_control.parse() {
            response.headers_mut().insert("Cache-Control", hv);
        }
    }
    Ok(response)
}

/// Builds the opt-in `Cache-Control` value for the active-entity listing endpoints, or `None` when
/// caching is disabled (`max_age == 0`).
pub(crate) fn entities_cache_control(max_age: u64) -> Option<String> {
    if max_age > 0 {
        Some(format!("public, max-age={}", max_age))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::entities_cache_control;

    #[test]
    fn cache_control_disabled_when_zero() {
        assert_eq!(entities_cache_control(0), None);
    }

    #[test]
    fn cache_control_set_when_positive() {
        assert_eq!(
            entities_cache_control(10).as_deref(),
            Some("public, max-age=10")
        );
    }
}
