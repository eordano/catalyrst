use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::errors::{AppError, AppResult, NotFoundError};
use crate::formatters::check_not_modified;
// X-Accel-Redirect zero-copy (PERF-B) is inherited via serve_content_blob:
// when STORAGE_X_ACCEL_BASE is set, the shared blob serializer returns an
// empty body + X-Accel-Redirect header so nginx sendfile()s the bytes.
use crate::handlers::get_entity_thumbnail::serve_content_blob;
use crate::state::AppState;

pub async fn get_entity_image(
    State(state): State<Arc<AppState>>,
    Path(pointer): Path<String>,
    method: Method,
    headers: HeaderMap,
) -> AppResult<Response> {
    let entity = state
        .database
        .find_entity_by_pointer(&pointer)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| NotFoundError::new("Entity not found."))?;

    let hash = extract_image_hash(&entity)
        .ok_or_else(|| NotFoundError::new("Entity has no image."))?;

    if let Some(not_modified_headers) = check_not_modified(&headers, &hash) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        let resp_headers = response.headers_mut();
        for (name, value) in not_modified_headers {
            if let Ok(hv) = value.parse() {
                resp_headers.insert(name, hv);
            }
        }
        return Ok(response);
    }

    serve_content_blob(&state, &hash, &method, &headers).await
}

fn extract_image_hash(entity: &serde_json::Value) -> Option<String> {
    let metadata = entity.get("metadata")?;
    let image_path = metadata.get("image")?.as_str()?;

    let content = entity.get("content")?.as_array()?;
    for item in content {
        let file = item.get("file").or_else(|| item.get("key"))?.as_str()?;
        if file == image_path {
            return item.get("hash").and_then(|h| h.as_str()).map(|s| s.to_string());
        }
    }

    None
}
