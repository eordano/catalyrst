use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::formatters::{mask_entity, EntityField};
use crate::query_params::{parse_query_string, qs_get_array, qs_get_string};
use crate::state::{retain_non_denylisted, AppState};
use crate::validation::{validate_ids_or_pointers, MAX_IDS_OR_POINTERS};

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

    let use_ids = validate_ids_or_pointers(
        (!ids.is_empty()).then_some(ids.as_slice()),
        (!pointers.is_empty()).then_some(pointers.as_slice()),
        MAX_IDS_OR_POINTERS,
    )?;

    let fields_param = qs_get_string(&params, "fields");
    let fields: Option<Vec<EntityField>> = fields_param.map(|f| {
        f.split(',')
            .filter_map(|s| EntityField::parse(s.trim()))
            .collect()
    });

    let mut entities: Vec<Value> = if use_ids {
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

    retain_non_denylisted(&mut entities, state.denylist.as_ref());

    let masked: Vec<Value> = entities
        .iter()
        .map(|e| mask_entity(e, fields.as_deref()))
        .collect();

    let mut response = Json(Value::Array(masked)).into_response();
    if let Some(cache_control) = entities_cache_control(state.entities_cache_control_max_age) {
        if let Ok(hv) = cache_control.parse() {
            response.headers_mut().insert("Cache-Control", hv);
        }
    }
    Ok(response)
}

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
    use crate::query_params::{parse_query_string, qs_get_array};
    use crate::validation::{validate_ids_or_pointers, MAX_IDS_OR_POINTERS};

    fn selector_use_ids(query_string: &str) -> Result<bool, crate::errors::AppError> {
        let params = parse_query_string(query_string);
        let pointers: Vec<String> = qs_get_array(&params, "pointer")
            .into_iter()
            .map(|p| p.to_lowercase())
            .collect();
        let ids = qs_get_array(&params, "id");
        validate_ids_or_pointers(
            (!ids.is_empty()).then_some(ids.as_slice()),
            (!pointers.is_empty()).then_some(pointers.as_slice()),
            MAX_IDS_OR_POINTERS,
        )
    }

    #[test]
    fn empty_pointer_query_element_is_rejected() {
        let err = selector_use_ids("pointer=").unwrap_err();
        assert!(err
            .to_string()
            .contains("None of the elements can be empty"));
    }

    #[test]
    fn valid_pointer_query_selects_pointers() {
        assert!(!selector_use_ids("pointer=0,0").unwrap());
    }

    #[test]
    fn valid_id_query_selects_ids() {
        assert!(selector_use_ids("id=bafkreitest").unwrap());
    }

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
