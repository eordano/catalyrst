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
use crate::handlers::get_content::{
    detect_content_type, set_content_type, x_accel_base, x_accel_redirect_path,
};
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

pub(crate) async fn serve_content_blob(
    state: &AppState,
    hash: &str,
    method: &Method,
    headers: &HeaderMap,
) -> AppResult<Response> {
    let mut opened = state
        .storage
        .open(hash)
        .await?
        .ok_or_else(|| NotFoundError::new("Content not found."))?;

    let head = opened.read_range(0, 31).await?;
    let detected = detect_content_type(&head);

    let range_header = headers.get("range").and_then(|v| v.to_str().ok());
    let total = opened.size;
    let range = parse_range_header(range_header, Some(total));
    let mut base_headers = content_file_headers(hash, Some(total), opened.encoding.as_deref());
    set_content_type(&mut base_headers, detected);

    match range {
        Some(ParsedRange::Unsatisfiable) => {
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
            let body: Bytes = if *method == Method::HEAD {
                Bytes::new()
            } else {
                opened.read_range(start, end).await?
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

            let body = if *method == Method::HEAD {
                Body::empty()
            } else {
                opened.into_body(head)
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
    use crate::state::{ContentStorage, FileInfo, OpenedContent};
    use async_trait::async_trait;
    use axum::body::to_bytes;
    use catalyrst_storage::StorageError;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const HASH: &str = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenosa7776";

    fn make_blob() -> Bytes {
        let mut blob = vec![0u8; 1024 * 1024];
        blob[..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        for (i, b) in blob.iter_mut().enumerate().skip(8) {
            *b = (i % 251) as u8;
        }
        Bytes::from(blob)
    }

    /// Records every read off the opened handle so a test can prove HEAD reads no body.
    struct RecordingStorage {
        blob: Bytes,
        opens: AtomicUsize,
        bytes_read: Arc<AtomicUsize>,
    }

    impl RecordingStorage {
        fn new(blob: Bytes) -> Arc<Self> {
            Arc::new(Self {
                blob,
                opens: AtomicUsize::new(0),
                bytes_read: Arc::new(AtomicUsize::new(0)),
            })
        }
    }

    struct CountingReader {
        inner: std::io::Cursor<Bytes>,
        bytes_read: Arc<AtomicUsize>,
    }

    impl tokio::io::AsyncRead for CountingReader {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let before = buf.filled().len();
            let res = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
            self.bytes_read
                .fetch_add(buf.filled().len() - before, Ordering::SeqCst);
            res
        }
    }

    impl tokio::io::AsyncSeek for CountingReader {
        fn start_seek(
            mut self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            std::pin::Pin::new(&mut self.inner).start_seek(position)
        }

        fn poll_complete(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<u64>> {
            std::pin::Pin::new(&mut self.inner).poll_complete(cx)
        }
    }

    #[async_trait]
    impl ContentStorage for RecordingStorage {
        async fn retrieve(&self, _hash: &str) -> Result<Option<Bytes>, StorageError> {
            panic!("serve_content_blob must go through open()");
        }

        async fn retrieve_stream(
            &self,
            _hash: &str,
        ) -> Result<Option<(axum::body::Body, u64)>, StorageError> {
            panic!("serve_content_blob must go through open()");
        }

        async fn retrieve_range(
            &self,
            _hash: &str,
            _start: u64,
            _end: u64,
        ) -> Result<Option<Bytes>, StorageError> {
            panic!("serve_content_blob must go through open()");
        }

        async fn file_info(&self, _hash: &str) -> Result<Option<FileInfo>, StorageError> {
            panic!("serve_content_blob must go through open()");
        }

        async fn exist_multiple(
            &self,
            _hashes: &[String],
        ) -> Result<HashMap<String, bool>, StorageError> {
            Ok(HashMap::new())
        }

        async fn open(&self, _hash: &str) -> Result<Option<OpenedContent>, StorageError> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Some(OpenedContent {
                size: self.blob.len() as u64,
                encoding: None,
                reader: Box::new(CountingReader {
                    inner: std::io::Cursor::new(self.blob.clone()),
                    bytes_read: self.bytes_read.clone(),
                }),
            }))
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
        let blob = make_blob();
        let size = blob.len() as u64;
        let storage = RecordingStorage::new(blob.clone());
        let state = crate::test_support::app_state_with_storage(storage.clone());

        let resp = serve_content_blob(&state, HASH, &Method::HEAD, &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ctype_head = header(&resp, "content-type").unwrap();
        assert_eq!(ctype_head, "image/png");
        assert_eq!(header(&resp, "content-length").unwrap(), size.to_string());
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
        assert_eq!(storage.opens.load(Ordering::SeqCst), 1);
        assert_eq!(storage.bytes_read.load(Ordering::SeqCst), 32);

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
        assert_eq!(storage.opens.load(Ordering::SeqCst), 2);
        assert_eq!(storage.bytes_read.load(Ordering::SeqCst), 64);

        let resp = serve_content_blob(&state, HASH, &Method::GET, &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "content-type").unwrap(), ctype_head);
        assert_eq!(header(&resp, "content-length").unwrap(), size.to_string());
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, blob);
        assert_eq!(storage.opens.load(Ordering::SeqCst), 3);
        assert_eq!(
            storage.bytes_read.load(Ordering::SeqCst),
            64 + size as usize
        );

        let resp = serve_content_blob(&state, HASH, &Method::GET, &headers)
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], &blob[5..=9]);
        assert_eq!(storage.opens.load(Ordering::SeqCst), 4);
    }
}
