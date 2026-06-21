use axum::extract::State;
use bytes::Bytes;
use serde_json::Value;

use crate::http::ApiError;
use crate::AppState;

const MAX_POINTERS: usize = 50;

pub async fn active_entities(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<axum::Json<Vec<Value>>, ApiError> {
    let parsed: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("Invalid request. Request body is not valid"))?;

    let pointers_value = parsed.get("pointers");
    let Some(Value::Array(items)) = pointers_value else {
        return Err(ApiError::bad_request(
            "Invalid request. Request body is not valid",
        ));
    };

    let mut pointers: Vec<String> = Vec::new();
    for item in items {
        if let Value::String(s) = item {
            if !s.is_empty() {
                pointers.push(s.clone());
            }
        }
    }

    if pointers.len() > MAX_POINTERS {
        return Err(ApiError::bad_request(format!(
            "Maximum {} pointers allowed per request",
            MAX_POINTERS
        )));
    }

    let mut seen = std::collections::HashSet::new();
    let unique: Vec<String> = pointers
        .into_iter()
        .filter(|p| seen.insert(p.to_lowercase()))
        .collect();

    let mut allowed: Vec<String> = Vec::with_capacity(unique.len());
    for pointer in unique {
        if state.name_denylist.check_name_deny_list(&pointer).await {
            allowed.push(pointer);
        }
    }

    let entities = state.worlds.get_entities_for_worlds(&allowed).await?;
    Ok(axum::Json(entities))
}
