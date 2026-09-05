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
        .await?
        .ok_or_else(|| NotFoundError::new("Content not found."))?;

    let detected = state
        .storage
        .retrieve_range(hash, 0, 31)
        .await?
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
            // HEAD needs only the headers (which come from `file_info` + the 32-byte sniff),
            // so skip the range read entirely rather than fetching bytes only to discard them.
            let body: Bytes = if *method == Method::HEAD {
                Bytes::new()
            } else {
                state
                    .storage
                    .retrieve_range(hash, start, end)
                    .await?
                    .ok_or_else(|| NotFoundError::new("Content not found."))?
            };

            let total = total_size.unwrap_or(0);
            let mut base_headers =
                content_file_headers(hash, file_info.size, file_info.encoding.as_deref());
            set_content_type(&mut base_headers, detected);

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

            // HEAD needs only the headers, so skip the whole-blob read on that path.
            let body: Bytes = if *method == Method::HEAD {
                Bytes::new()
            } else {
                state
                    .storage
                    .retrieve(hash)
                    .await?
                    .ok_or_else(|| NotFoundError::new("Content not found."))?
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

#[cfg(test)]
mod head_body_tests {
    use super::*;
    use crate::state::{ContentStorage, FileInfo};
    use async_trait::async_trait;
    use axum::body::to_bytes;
    use catalyrst_storage::StorageError;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const HASH: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    fn make_blob() -> Bytes {
        // 1 MiB; first 8 bytes are the PNG signature so the 32-byte sniff yields "image/png".
        let mut blob = vec![0u8; 1024 * 1024];
        blob[..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        for (i, b) in blob.iter_mut().enumerate().skip(8) {
            *b = (i % 251) as u8;
        }
        Bytes::from(blob)
    }

    /// Records every `retrieve`/`retrieve_range` call so a test can prove HEAD reads no body.
    struct RecordingStorage {
        blob: Bytes,
        retrieve_calls: AtomicUsize,
        ranges: Mutex<Vec<(u64, u64)>>,
    }

    impl RecordingStorage {
        fn new(blob: Bytes) -> Arc<Self> {
            Arc::new(Self {
                blob,
                retrieve_calls: AtomicUsize::new(0),
                ranges: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl ContentStorage for RecordingStorage {
        async fn retrieve(&self, _hash: &str) -> Result<Option<Bytes>, StorageError> {
            self.retrieve_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(self.blob.clone()))
        }

        async fn retrieve_stream(
            &self,
            _hash: &str,
        ) -> Result<Option<(axum::body::Body, u64)>, StorageError> {
            Ok(None)
        }

        async fn retrieve_range(
            &self,
            _hash: &str,
            start: u64,
            end: u64,
        ) -> Result<Option<Bytes>, StorageError> {
            self.ranges.lock().unwrap().push((start, end));
            let len = self.blob.len() as u64;
            let end = end.min(len.saturating_sub(1));
            if start > end || start >= len {
                return Ok(None);
            }
            Ok(Some(self.blob.slice(start as usize..=end as usize)))
        }

        async fn file_info(&self, _hash: &str) -> Result<Option<FileInfo>, StorageError> {
            let len = self.blob.len() as u64;
            Ok(Some(FileInfo {
                size: Some(len),
                content_size: Some(len),
                encoding: None,
            }))
        }

        async fn exist_multiple(
            &self,
            _hashes: &[String],
        ) -> Result<HashMap<String, bool>, StorageError> {
            Ok(HashMap::new())
        }
    }

    fn header(resp: &Response, name: &str) -> Option<String> {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    #[tokio::test]
    async fn head_serve_content_blob_skips_body_reads() {
        // NB: this test never sets STORAGE_X_ACCEL_BASE, so the None branch reaches `retrieve`.
        let blob = make_blob();
        let size = blob.len() as u64;
        let storage = RecordingStorage::new(blob.clone());
        let state = crate::test_support::app_state_with_storage(storage.clone());

        // (a) HEAD, no Range -> 200, empty body, headers from file_info + sniff, zero retrieve().
        let resp = serve_content_blob(&state, HASH, &Method::HEAD, &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ctype_head = header(&resp, "content-type").unwrap();
        assert_eq!(ctype_head, "image/png");
        assert_eq!(header(&resp, "content-length").unwrap(), size.to_string());
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
        assert_eq!(storage.retrieve_calls.load(Ordering::SeqCst), 0);
        assert_eq!(&*storage.ranges.lock().unwrap(), &[(0, 31)]);

        // (b) HEAD with a Range -> 206, empty body, correct Content-Range/Length, still no read.
        let mut headers = HeaderMap::new();
        headers.insert("range", "bytes=5-9".parse().unwrap());
        let resp = serve_content_blob(&state, HASH, &Method::HEAD, &headers)
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            header(&resp, "content-range").unwrap(),
            format!("bytes 5-9/{size}")
        );
        assert_eq!(header(&resp, "content-length").unwrap(), "5");
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
        assert_eq!(storage.retrieve_calls.load(Ordering::SeqCst), 0);
        assert!(storage.ranges.lock().unwrap().iter().all(|&w| w == (0, 31)));

        // (c) GET, no Range -> full body byte-for-byte, Content-Type identical, one retrieve().
        let resp = serve_content_blob(&state, HASH, &Method::GET, &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "content-type").unwrap(), ctype_head);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, blob);
        assert_eq!(storage.retrieve_calls.load(Ordering::SeqCst), 1);

        // (d) GET with a Range -> exactly blob[5..=9], and the data window is now read.
        let resp = serve_content_blob(&state, HASH, &Method::GET, &headers)
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], &blob[5..=9]);
        assert!(storage.ranges.lock().unwrap().contains(&(5, 9)));
    }
}
