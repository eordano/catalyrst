use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::errors::{AppError, AppResult, NotFoundError};
use crate::formatters::{
    check_not_modified, content_file_headers, parse_range_header, ParsedRange,
};
use crate::handlers::get_content::{x_accel_base, x_accel_redirect_path};
use crate::state::AppState;

pub async fn get_entity_thumbnail(
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

    let entity_id = entity.get("id").and_then(|id| id.as_str()).unwrap_or("");
    if state.denylist.is_denylisted(entity_id) {
        return Err(NotFoundError::new("Entity not found.").into());
    }

    let hash = extract_thumbnail_hash(&entity)
        .ok_or_else(|| NotFoundError::new("Entity has no thumbnail."))?;

    if state.denylist.is_denylisted(&hash) {
        return Err(NotFoundError::new("Entity has no thumbnail.").into());
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

fn extract_thumbnail_hash(entity: &serde_json::Value) -> Option<String> {
    let metadata = entity.get("metadata")?;
    let thumbnail_path = metadata.get("thumbnail")?.as_str()?;

    let content = entity.get("content")?.as_array()?;
    for item in content {
        let file = item.get("file").or_else(|| item.get("key"))?.as_str()?;
        if file == thumbnail_path {
            return item
                .get("hash")
                .and_then(|h| h.as_str())
                .map(|s| s.to_string());
        }
    }

    None
}

fn set_content_type(headers: &mut [(&'static str, String)], mime: &str) {
    for (name, value) in headers.iter_mut() {
        if *name == "Content-Type" {
            *value = mime.to_string();
        }
    }
}

pub(crate) async fn serve_content_blob(
    state: &AppState,
    hash: &str,
    method: &Method,
    headers: &HeaderMap,
) -> AppResult<Response> {
    let file_info = state
        .storage
        .file_info(hash)
        .await
        .ok_or_else(|| NotFoundError::new("Content not found."))?;

    let detected = state
        .storage
        .retrieve_range(hash, 0, 31)
        .await
        .map(|head| crate::handlers::get_content::detect_content_type(&head))
        .unwrap_or("application/octet-stream");

    let range_header = headers.get("range").and_then(|v| v.to_str().ok());
    let total_size = file_info.content_size.or(file_info.size);
    let range = parse_range_header(range_header, total_size);

    match range {
        Some(ParsedRange::Unsatisfiable) => {
            let total = total_size.unwrap_or(0);
            let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            let resp_headers = response.headers_mut();
            if let Ok(hv) = format!("bytes */{}", total).parse() {
                resp_headers.insert("Content-Range", hv);
            }
            if let Ok(hv) = "Content-Range".parse() {
                resp_headers.insert("Access-Control-Expose-Headers", hv);
            }
            Ok(response)
        }
        Some(ParsedRange::Range { start, end }) => {
            let content = state
                .storage
                .retrieve_range(hash, start, end)
                .await
                .ok_or_else(|| NotFoundError::new("Content not found."))?;

            let total = total_size.unwrap_or(0);
            let mut base_headers =
                content_file_headers(hash, file_info.size, file_info.encoding.as_deref());
            set_content_type(&mut base_headers, detected);

            let body: Bytes = if *method == Method::HEAD {
                Bytes::new()
            } else {
                content
            };

            let content_len = end - start + 1;
            let mut response = (StatusCode::PARTIAL_CONTENT, body).into_response();
            let resp_headers = response.headers_mut();
            for (name, value) in &base_headers {
                if let Ok(hv) = value.parse() {
                    resp_headers.insert(*name, hv);
                }
            }
            if let Ok(hv) = format!("bytes {}-{}/{}", start, end, total).parse() {
                resp_headers.insert("Content-Range", hv);
            }
            if let Ok(hv) = content_len.to_string().parse() {
                resp_headers.insert("Content-Length", hv);
            }
            Ok(response)
        }
        None => {
            let mut base_headers =
                content_file_headers(hash, file_info.size, file_info.encoding.as_deref());
            set_content_type(&mut base_headers, detected);

            if let Some(accel) = x_accel_base().and_then(|b| x_accel_redirect_path(Some(&b), hash))
            {
                base_headers.retain(|(n, _)| *n != "Content-Length");
                let mut response = (StatusCode::OK, Body::empty()).into_response();
                let resp_headers = response.headers_mut();
                for (name, value) in &base_headers {
                    if let Ok(hv) = value.parse() {
                        resp_headers.insert(*name, hv);
                    }
                }
                if let Ok(hv) = accel.parse() {
                    resp_headers.insert("X-Accel-Redirect", hv);
                }
                if let Ok(hv) = "0".parse() {
                    resp_headers.insert("Content-Length", hv);
                }
                return Ok(response);
            }

            let content = state
                .storage
                .retrieve(hash)
                .await
                .ok_or_else(|| NotFoundError::new("Content not found."))?;

            let body: Bytes = if *method == Method::HEAD {
                Bytes::new()
            } else {
                content
            };

            let mut response = (StatusCode::OK, body).into_response();
            let resp_headers = response.headers_mut();
            for (name, value) in &base_headers {
                if let Ok(hv) = value.parse() {
                    resp_headers.insert(*name, hv);
                }
            }
            Ok(response)
        }
    }
}
