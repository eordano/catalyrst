use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::http::errors::ApiError;
use crate::AppState;

pub async fn contracts_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let addr = address.to_lowercase();
    if state.contracts.is_valid_address(&addr).await? {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(ApiError::NotFound("Address is not valid".into()))
    }
}
