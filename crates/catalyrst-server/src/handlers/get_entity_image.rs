use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::errors::{AppError, AppResult, NotFoundError};
use crate::formatters::check_not_modified;

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

    // Treat a denylisted entity as not found so its content (and its very existence) isn't exposed
    // through the image endpoint, mirroring the listing endpoints that already filter the denylist.
    let entity_id = entity.get("id").and_then(|id| id.as_str()).unwrap_or("");
    if state.denylist.is_denylisted(entity_id) {
        return Err(NotFoundError::new("Entity not found.").into());
    }

    let hash =
        extract_image_hash(&entity).ok_or_else(|| NotFoundError::new("Entity has no image."))?;

    // Also guard against a specific content hash being denylisted independently of its entity.
    if state.denylist.is_denylisted(&hash) {
        return Err(NotFoundError::new("Entity has no image.").into());
    }

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
            return item
                .get("hash")
                .and_then(|h| h.as_str())
                .map(|s| s.to_string());
        }
    }

    None
}
