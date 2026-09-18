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
        if &first_bytes[4..8] == b"ftyp" {
            return "video/mp4";
        }
    }
    if first_bytes.len() >= 3 && first_bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg";
    }
    if first_bytes.len() >= 4 {
        if first_bytes.starts_with(b"RIFF")
            && first_bytes.len() >= 12
            && &first_bytes[8..12] == b"WEBP"
        {
            return "image/webp";
        }
        if first_bytes.starts_with(b"OggS") {
            return "audio/ogg";
        }
        if first_bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
            return "video/webm";
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

pub(crate) fn x_accel_redirect_path(base: Option<&str>, hash: &str) -> Option<String> {
    let base = base?.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }

    let shard = catalyrst_storage::hex_prefix(hash);
    Some(format!("{base}/{shard}/{hash}"))
}

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
    if !catalyrst_hashing::is_canonical_cid(&hash_id) {
        return Err(NotFoundError::new(format!("No content found with hash {}", hash_id)).into());
    }

    if state.denylist.is_denylisted(&hash_id) {
        return Err(NotFoundError::new(format!("No content found with hash {}", hash_id)).into());
    }

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

    let mut opened = state
        .storage
        .open(&hash_id)
        .await?
        .ok_or_else(|| NotFoundError::new(format!("No content found with hash {}", hash_id)))?;

    let range_header = headers.get("range").and_then(|v| v.to_str().ok());
    let total = opened.size;
    let range = parse_range_header(range_header, Some(total));
    let mut base_headers = content_file_headers(&hash_id, Some(total), opened.encoding.as_deref());

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
            return Ok(response);
        }
        Some(ParsedRange::Range { start, end }) => {
            let content = if method == Method::HEAD && query.include_mime_type.is_none() {
                Bytes::new()
            } else {
                opened.read_range(start, end).await?
            };

            if query.include_mime_type.is_some() {
                set_content_type(&mut base_headers, detect_content_type(&content));
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
        None => {}
    }

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

    let head = if query.include_mime_type.is_some() {
        let head = opened.read_range(0, 31).await?;
        set_content_type(&mut base_headers, detect_content_type(&head));
        head
    } else {
        Bytes::new()
    };

    if let Some(accel) = x_accel_base().and_then(|b| x_accel_redirect_path(Some(&b), &hash_id)) {
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

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(opened.into_body(head))
        .unwrap();
    let resp_headers = response.headers_mut();
    for (name, value) in &base_headers {
        if let Ok(hv) = value.parse() {
            resp_headers.insert(*name, hv);
        }
    }
    Ok(response)
}

pub(crate) fn set_content_type(headers: &mut [(&'static str, String)], mime: &str) {
    for (name, value) in headers.iter_mut() {
        if *name == "Content-Type" {
            *value = mime.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_accel_redirect_path_none_when_base_unset() {
        assert_eq!(
            x_accel_redirect_path(None, "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"),
            None
        );
    }

    #[test]
    fn x_accel_redirect_path_none_when_base_empty() {
        assert_eq!(
            x_accel_redirect_path(Some(""), "QmcoQSrVoi8CKSwiRyJ3MPYyN1AUiLjHiAtYCUGoBr8JM4"),
            None
        );
    }

    #[test]
    fn x_accel_redirect_path_uses_storage_shard() {
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

    #[test]
    fn detect_content_type_sniffs_video() {
        assert_eq!(
            detect_content_type(&[0x1A, 0x45, 0xDF, 0xA3, 0x01, 0x00]),
            "video/webm"
        );
        assert_eq!(
            detect_content_type(b"\0\0\0\x20ftypisom\0\0\x02\0"),
            "video/mp4"
        );
        assert_eq!(
            detect_content_type(b"plain bytes"),
            "application/octet-stream"
        );
    }

    const TEST_CID: &str = "bafkreie4eisvkzyjuqrcendydk6vikqs2vco5lmib4nlzsxtjzofiqy2pa";

    async fn get(state: Arc<crate::state::AppState>) -> AppResult<Response> {
        get_content(
            State(state),
            Path(TEST_CID.to_string()),
            Query(ContentQuery {
                include_mime_type: None,
            }),
            Method::GET,
            HeaderMap::new(),
        )
        .await
    }

    #[tokio::test]
    async fn storage_fault_answers_500_not_404() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::FaultyStorage,
        ));
        let err = get(state)
            .await
            .expect_err("a storage fault must error, not resolve");
        assert_eq!(
            err.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "a broken node must error (and get retried), not advertise the content as absent"
        );
    }

    #[tokio::test]
    async fn missing_content_still_answers_404() {
        let state = crate::test_support::app_state_with_storage(Arc::new(
            crate::test_support::EmptyStorage,
        ));
        let err = get(state).await.expect_err("missing content is a 404");
        assert_eq!(err.into_response().status(), StatusCode::NOT_FOUND);
    }

    struct BlobStorage(Bytes);

    #[async_trait::async_trait]
    impl crate::state::ContentStorage for BlobStorage {
        async fn retrieve(
            &self,
            _hash: &str,
        ) -> Result<Option<Bytes>, catalyrst_storage::StorageError> {
            panic!("get_content must go through open()");
        }

        async fn retrieve_stream(
            &self,
            _hash: &str,
        ) -> Result<Option<(Body, u64)>, catalyrst_storage::StorageError> {
            panic!("get_content must go through open()");
        }

        async fn retrieve_range(
            &self,
            _hash: &str,
            _start: u64,
            _end: u64,
        ) -> Result<Option<Bytes>, catalyrst_storage::StorageError> {
            panic!("get_content must go through open()");
        }

        async fn file_info(
            &self,
            _hash: &str,
        ) -> Result<Option<crate::state::FileInfo>, catalyrst_storage::StorageError> {
            panic!("get_content must go through open()");
        }

        async fn exist_multiple(
            &self,
            _hashes: &[String],
        ) -> Result<std::collections::HashMap<String, bool>, catalyrst_storage::StorageError>
        {
            Ok(Default::default())
        }

        async fn open(
            &self,
            _hash: &str,
        ) -> Result<Option<crate::state::OpenedContent>, catalyrst_storage::StorageError> {
            Ok(Some(crate::state::OpenedContent {
                size: self.0.len() as u64,
                encoding: None,
                reader: Box::new(std::io::Cursor::new(self.0.clone())),
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
    async fn include_mime_type_sniffs_and_replays_the_head() {
        let mut blob = vec![0u8; 200 * 1024];
        blob[..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        for (i, b) in blob.iter_mut().enumerate().skip(8) {
            *b = (i % 251) as u8;
        }
        let blob = Bytes::from(blob);
        let state =
            crate::test_support::app_state_with_storage(Arc::new(BlobStorage(blob.clone())));
        let run = |method: Method, mime: Option<&str>, range: Option<&str>| {
            let state = state.clone();
            let mime = mime.map(str::to_string);
            let mut headers = HeaderMap::new();
            if let Some(r) = range {
                headers.insert("range", r.parse().unwrap());
            }
            async move {
                get_content(
                    State(state),
                    Path(TEST_CID.to_string()),
                    Query(ContentQuery {
                        include_mime_type: mime,
                    }),
                    method,
                    headers,
                )
                .await
                .unwrap()
            }
        };

        let resp = run(Method::GET, Some(""), None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "content-type").unwrap(), "image/png");
        assert_eq!(
            header(&resp, "content-length").unwrap(),
            blob.len().to_string()
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, blob);

        let resp = run(Method::GET, None, None).await;
        assert_eq!(
            header(&resp, "content-type").unwrap(),
            "application/octet-stream"
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, blob);

        let resp = run(Method::HEAD, Some(""), None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body.is_empty());

        let resp = run(Method::GET, Some(""), Some("bytes=0-9")).await;
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(header(&resp, "content-type").unwrap(), "image/png");
        assert_eq!(
            header(&resp, "content-range").unwrap(),
            format!("bytes 0-9/{}", blob.len())
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], &blob[..10]);

        let resp = run(Method::GET, None, Some("bytes=999999999-")).await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }
}
