use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Deserialize;

use crate::errors::{AppResult, NotFoundError};
use crate::formatters::{
    check_not_modified, content_file_headers, parse_range_header, ParsedRange,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ContentQuery {
    #[serde(rename = "includeMimeType")]
    pub include_mime_type: Option<String>,
}

pub fn detect_content_type(first_bytes: &[u8]) -> &'static str {
    if first_bytes.len() >= 8 {
        if first_bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return "image/png";
        }
        if first_bytes.starts_with(&[0x67, 0x6C, 0x54, 0x46]) {
            return "model/gltf-binary";
        }
    }
    if first_bytes.len() >= 3
        && first_bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return "image/jpeg";
        }
    if first_bytes.len() >= 4 {
        if first_bytes.starts_with(b"RIFF") && first_bytes.len() >= 12 && &first_bytes[8..12] == b"WEBP" {
            return "image/webp";
        }
        if first_bytes.starts_with(b"OggS") {
            return "audio/ogg";
        }
    }
    let trimmed = if first_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &first_bytes[3..]
    } else {
        first_bytes
    };
    if let Some(&first) = trimmed.first() {
        if first == b'{' || first == b'[' {
            return "application/json";
        }
    }

    "application/octet-stream"
}

/// Build the X-Accel-Redirect internal path for `hash` given an optional base.
///
/// Returns `None` when no base is configured (Rust serves the body itself).
/// The shard scheme matches `catalyrst_storage::hex_prefix` (sha1[..2]).
pub(crate) fn x_accel_redirect_path(base: Option<&str>, hash: &str) -> Option<String> {
    let base = base?.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    // matches catalyrst_storage::resolve_file_path sharding
    let shard = catalyrst_storage::hex_prefix(hash);
    Some(format!("{base}/{shard}/{hash}"))
}

/// Returns the configured X-Accel-Redirect base (`STORAGE_X_ACCEL_BASE`), or
/// `None` when the env var is unset/empty — in which case Rust streams the
/// body itself, matching pre-PERF-B behavior.
pub(crate) fn x_accel_base() -> Option<String> {
    let v = std::env::var("STORAGE_X_ACCEL_BASE").ok()?;
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

pub async fn get_content(
    State(state): State<Arc<AppState>>,
    Path(hash_id): Path<String>,
    Query(query): Query<ContentQuery>,
    method: Method,
    headers: HeaderMap,
) -> AppResult<Response> {

    if let Some(not_modified_headers) = check_not_modified(&headers, &hash_id) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        let resp_headers = response.headers_mut();
        for (name, value) in not_modified_headers {
            if let Ok(hv) = value.parse() {
                resp_headers.insert(name, hv);
            }
        }
        return Ok(response);
    }

    let file_info = state
        .storage
        .file_info(&hash_id)
        .await
        .ok_or_else(|| NotFoundError::new(format!("No content found with hash {}", hash_id)))?;

    let range_header = headers
        .get("range")
        .and_then(|v| v.to_str().ok());
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
            return Ok(response);
        }
        Some(ParsedRange::Range { start, end }) => {
            let content = state
                .storage
                .retrieve_range(&hash_id, start, end)
                .await
                .ok_or_else(|| NotFoundError::new(format!("No content found with hash {}", hash_id)))?;

            let total = total_size.unwrap_or(0);
            let mut base_headers = content_file_headers(
                &hash_id,
                file_info.size,
                file_info.encoding.as_deref(),
            );

            if query.include_mime_type.is_some() {
                let detected = detect_content_type(&content);
                for (name, value) in &mut base_headers {
                    if *name == "Content-Type" {
                        *value = detected.to_string();
                    }
                }
            }

            let body: Bytes = if method == Method::HEAD {
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
            return Ok(response);
        }
        None => {
        }
    }

    let mut base_headers = content_file_headers(
        &hash_id,
        file_info.size,
        file_info.encoding.as_deref(),
    );

    if method == Method::HEAD {
        let mut response = StatusCode::OK.into_response();
        let resp_headers = response.headers_mut();
        for (name, value) in &base_headers {
            if let Ok(hv) = value.parse() {
                resp_headers.insert(*name, hv);
            }
        }
        return Ok(response);
    }

    // X-Accel-Redirect zero-copy path: when STORAGE_X_ACCEL_BASE is set, hand
    // the byte transfer to nginx. We still need to sniff the first bytes when
    // includeMimeType=true so the Content-Type header is correct.
    if let Some(accel) = x_accel_base().and_then(|b| x_accel_redirect_path(Some(&b), &hash_id)) {
        if query.include_mime_type.is_some() {
            if let Some(head) = state.storage.retrieve_range(&hash_id, 0, 31).await {
                let detected = detect_content_type(&head);
                for (name, value) in &mut base_headers {
                    if *name == "Content-Type" {
                        *value = detected.to_string();
                    }
                }
            }
        }
        // nginx serves the body — we send 0 bytes. Drop any Content-Length we
        // computed for the on-disk size; nginx will set it from the file.
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

    if let Some((body, on_disk_size)) = state.storage.retrieve_stream(&hash_id).await {
        if query.include_mime_type.is_some() {
        } else {
            for (name, value) in &mut base_headers {
                if *name == "Content-Length" {
                    *value = on_disk_size.to_string();
                }
            }

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(body)
                .unwrap();

            let resp_headers = response.headers_mut();
            for (name, value) in &base_headers {
                if let Ok(hv) = value.parse() {
                    resp_headers.insert(*name, hv);
                }
            }
            return Ok(response);
        }
    }

    let content = state
        .storage
        .retrieve(&hash_id)
        .await
        .ok_or_else(|| NotFoundError::new(format!("No content found with hash {}", hash_id)))?;

    if query.include_mime_type.is_some() {
        let detected = detect_content_type(&content);
        for (name, value) in &mut base_headers {
            if *name == "Content-Type" {
                *value = detected.to_string();
            }
        }
    }

    let mut response = (StatusCode::OK, Body::from(content)).into_response();
    let resp_headers = response.headers_mut();
    for (name, value) in &base_headers {
        if let Ok(hv) = value.parse() {
            resp_headers.insert(*name, hv);
        }
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_accel_redirect_path_none_when_base_unset() {
        assert_eq!(x_accel_redirect_path(None, "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"), None);
    }

    #[test]
    fn x_accel_redirect_path_none_when_base_empty() {
        assert_eq!(x_accel_redirect_path(Some(""), "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"), None);
    }

    #[test]
    fn x_accel_redirect_path_uses_storage_shard() {
        // sha1("QmcoQSrVoi8C...")[..2] = "f0" "49" → "f049"
        let got = x_accel_redirect_path(
            Some("/__protected_storage"),
            "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4",
        );
        assert_eq!(
            got.as_deref(),
            Some("/__protected_storage/f049/QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"),
        );
    }

    #[test]
    fn x_accel_redirect_path_strips_trailing_slash() {
        let got = x_accel_redirect_path(
            Some("/__protected_storage/"),
            "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa",
        );
        assert_eq!(
            got.as_deref(),
            Some("/__protected_storage/f049/bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa"),
        );
    }
}
