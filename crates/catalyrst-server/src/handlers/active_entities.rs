use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

use crate::errors::{AppError, AppResult, InvalidRequestError};
use crate::state::AppState;

const MAX_IDS_OR_POINTERS: usize = 1000;

#[derive(Debug, serde::Deserialize)]
pub struct ActiveEntitiesRequest {
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    #[serde(default)]
    pub pointers: Option<Vec<String>>,
}

fn validate_active_entities_request(
    ids: Option<&[String]>,
    pointers: Option<&[String]>,
) -> Result<bool, AppError> {
    let (len, use_ids) = match (ids, pointers) {
        (Some(ids), None) if !ids.is_empty() && ids.iter().all(|s| !s.is_empty()) => {
            (ids.len(), true)
        }
        (None, Some(pointers))
            if !pointers.is_empty() && pointers.iter().all(|s| !s.is_empty()) =>
        {
            (pointers.len(), false)
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

    if len > MAX_IDS_OR_POINTERS {
        return Err(InvalidRequestError::new(format!(
            "Too many ids or pointers; the maximum allowed is {}",
            MAX_IDS_OR_POINTERS
        ))
        .into());
    }

    Ok(use_ids)
}

pub async fn get_active_entities(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveEntitiesRequest>,
) -> AppResult<impl IntoResponse> {
    let use_ids = validate_active_entities_request(body.ids.as_deref(), body.pointers.as_deref())?;
    let values = if use_ids {
        body.ids
            .as_ref()
            .expect("validate_active_entities_request guarantees ids is present")
    } else {
        body.pointers
            .as_ref()
            .expect("validate_active_entities_request guarantees pointers is present")
    };

    let entities = if use_ids {
        state
            .database
            .active_entities_by_ids(values)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        state
            .database
            .active_entities_by_pointers(values)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
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

    Ok(Json(filtered))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn accepts_ids_only() {
        let ids = strings(&["a", "b"]);
        assert!(validate_active_entities_request(Some(&ids), None).unwrap());
    }

    #[test]
    fn accepts_pointers_only() {
        let ptrs = strings(&["0,0"]);
        assert!(!validate_active_entities_request(None, Some(&ptrs)).unwrap());
    }

    #[test]
    fn rejects_both_present() {
        let ids = strings(&["a"]);
        let ptrs = strings(&["0,0"]);
        let err = validate_active_entities_request(Some(&ids), Some(&ptrs)).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("but not both"));
    }

    #[test]
    fn rejects_neither_present() {
        let err = validate_active_entities_request(None, None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("at least one element"));
    }

    #[test]
    fn rejects_empty_ids_array() {
        let empty: Vec<String> = Vec::new();
        let err = validate_active_entities_request(Some(&empty), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err.to_string().contains("at least one element"));
    }

    #[test]
    fn rejects_empty_string_element_in_ids() {
        let ids = strings(&["a", "", "c"]);
        let err = validate_active_entities_request(Some(&ids), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err
            .to_string()
            .contains("None of the elements can be empty"));
    }

    #[test]
    fn rejects_empty_string_element_in_pointers() {
        let ptrs = strings(&[""]);
        let err = validate_active_entities_request(None, Some(&ptrs)).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert!(err
            .to_string()
            .contains("None of the elements can be empty"));
    }

    #[test]
    fn accepts_exactly_1000_ids() {
        let ids: Vec<String> = (0..MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(validate_active_entities_request(Some(&ids), None).unwrap());
    }

    #[test]
    fn rejects_over_1000_ids() {
        let ids: Vec<String> = (0..=MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        assert_eq!(ids.len(), 1001);
        let err = validate_active_entities_request(Some(&ids), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidRequest(_)));
        assert_eq!(
            err.to_string(),
            "Too many ids or pointers; the maximum allowed is 1000"
        );
    }

    #[test]
    fn rejects_over_1000_pointers() {
        let ptrs: Vec<String> = (0..=MAX_IDS_OR_POINTERS).map(|i| i.to_string()).collect();
        let err = validate_active_entities_request(None, Some(&ptrs)).unwrap_err();
        assert!(err.to_string().contains("Too many ids or pointers"));
    }
}
