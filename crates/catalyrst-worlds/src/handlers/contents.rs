use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::http::ApiError;
use crate::AppState;

const FORWARD_REQ_HEADERS: &[&str] = &["range", "if-none-match", "if-modified-since"];

const EXPOSED_HEADERS: &str = "ETag, Accept-Ranges, Content-Range";

const IMMUTABLE_CACHE_CONTROL: &str = "public,max-age=31536000,s-maxage=31536000,immutable";

const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

const MIME_SNIFF_BYTES: u64 = 4100;

fn is_ipfs_v2(hash: &str) -> bool {
    hash.len() == 59 && hash.starts_with("ba") && hash.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn is_sha256_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn is_retrievable_content_key(hash: &str) -> bool {
    is_ipfs_v2(hash) || is_sha256_hex(hash)
}

async fn detect_content_type(path: &std::path::Path) -> String {
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return DEFAULT_CONTENT_TYPE.to_string(),
    };
    let mut head = Vec::with_capacity(MIME_SNIFF_BYTES as usize);
    if file
        .take(MIME_SNIFF_BYTES)
        .read_to_end(&mut head)
        .await
        .is_err()
    {
        return DEFAULT_CONTENT_TYPE.to_string();
    }
    sniff_content_type(&head).to_string()
}

fn sniff_content_type(head: &[u8]) -> &'static str {
    infer::get(head)
        .map(|t| t.mime_type())
        .unwrap_or(DEFAULT_CONTENT_TYPE)
}

fn parse_range(header: &str, size: u64) -> Option<Option<(u64, u64)>> {
    let spec = header.strip_prefix("bytes=")?;
    let (lhs, rhs) = spec.split_once('-')?;
    if rhs.contains('-') {
        return None;
    }
    let has_start = !lhs.is_empty();
    let has_end = !rhs.is_empty();
    if !has_start && !has_end {
        return None;
    }
    let (start, end) = if !has_start {
        let suffix: u64 = rhs.parse().ok()?;
        if suffix == 0 {
            return Some(None);
        }
        (size.saturating_sub(suffix), size.saturating_sub(1))
    } else {
        let start: u64 = lhs.parse().ok()?;
        let end: u64 = if has_end {
            rhs.parse().ok()?
        } else {
            size.saturating_sub(1)
        };
        (start, end)
    };
    if start >= size || end < start {
        return Some(None);
    }
    Some(Some((start, end.min(size - 1))))
}

const FORWARD_RESP_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
    "etag",
    "last-modified",
    "cache-control",
];

pub async fn get_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::GET).await
}

pub async fn head_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::HEAD).await
}

pub async fn available_content(
    State(state): State<AppState>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> Result<axum::Json<Vec<serde_json::Value>>, ApiError> {
    let cids: Vec<&str> = query
        .as_deref()
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.strip_prefix("cid="))
        .filter(|c| !c.is_empty())
        .collect();
    let mut out = Vec::with_capacity(cids.len());
    for cid in cids {
        let available = is_retrievable_content_key(cid)
            && tokio::fs::metadata(state.cfg.contents_dir.join(cid))
                .await
                .map(|m| m.is_file())
                .unwrap_or(false);
        out.push(serde_json::json!({ "cid": cid, "available": available }));
    }
    Ok(axum::Json(out))
}

async fn proxy(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    headers: HeaderMap,
    method: Method,
) -> Result<Response, ApiError> {
    if !is_retrievable_content_key(&hash) {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }

    let local = state.cfg.contents_dir.join(&hash);
    if let Ok(meta) = tokio::fs::metadata(&local).await {
        if meta.is_file() {
            let size = meta.len();
            let range = headers
                .get("range")
                .and_then(|v| v.to_str().ok())
                .and_then(|r| parse_range(r, size));
            match range {
                Some(None) => {
                    return Ok(Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header("content-range", format!("bytes */{size}"))
                        .header("accept-ranges", "bytes")
                        .body(Body::empty())
                        .unwrap());
                }
                Some(Some((start, end))) => {
                    let content_type = detect_content_type(&local).await;
                    let builder = Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header("content-type", content_type)
                        .header("content-range", format!("bytes {start}-{end}/{size}"))
                        .header("content-length", end - start + 1)
                        .header("etag", format!("\"{hash}\""))
                        .header("cache-control", IMMUTABLE_CACHE_CONTROL)
                        .header("access-control-expose-headers", EXPOSED_HEADERS)
                        .header("accept-ranges", "bytes");
                    if method == Method::HEAD {
                        return Ok(builder.body(Body::empty()).unwrap());
                    }
                    let mut file = tokio::fs::File::open(&local)
                        .await
                        .map_err(|e| ApiError::internal(format!("local content open: {e}")))?;
                    file.seek(std::io::SeekFrom::Start(start))
                        .await
                        .map_err(|e| ApiError::internal(format!("local content seek: {e}")))?;
                    let stream = ReaderStream::new(file.take(end - start + 1));
                    return Ok(builder.body(Body::from_stream(stream)).unwrap());
                }
                None => {}
            }
            let content_type = detect_content_type(&local).await;
            let builder = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", content_type)
                .header("content-length", size)
                .header("etag", format!("\"{hash}\""))
                .header("cache-control", IMMUTABLE_CACHE_CONTROL)
                .header("access-control-expose-headers", EXPOSED_HEADERS)
                .header("accept-ranges", "bytes");
            if method == Method::HEAD {
                return Ok(builder.body(Body::empty()).unwrap());
            }
            let file = tokio::fs::File::open(&local)
                .await
                .map_err(|e| ApiError::internal(format!("local content open: {e}")))?;
            return Ok(builder
                .body(Body::from_stream(ReaderStream::new(file)))
                .unwrap());
        }
    }

    let url = format!("{}/contents/{}", state.cfg.contents_upstream_url, hash);

    let mut req = match method {
        Method::HEAD => state.http.head(&url),
        _ => state.http.get(&url),
    };
    for name in FORWARD_REQ_HEADERS {
        if let Some(v) = headers.get(*name) {
            req = req.header(*name, v);
        }
    }

    let upstream = req
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("contents upstream error: {e}")))?;

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut out_headers = HeaderMap::new();
    for name in FORWARD_RESP_HEADERS {
        if let Some(v) = upstream.headers().get(*name) {
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                out_headers.insert(hn, hv);
            }
        }
    }

    if status.is_success() {
        out_headers.insert(
            HeaderName::from_static("access-control-expose-headers"),
            HeaderValue::from_static(EXPOSED_HEADERS),
        );
        if !out_headers.contains_key("cache-control") {
            out_headers.insert(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static(IMMUTABLE_CACHE_CONTROL),
            );
        }
        if !out_headers.contains_key("etag") {
            if let Ok(hv) = HeaderValue::from_str(&format!("\"{hash}\"")) {
                out_headers.insert(HeaderName::from_static("etag"), hv);
            }
        }
        if !out_headers.contains_key("accept-ranges") {
            out_headers.insert(
                HeaderName::from_static("accept-ranges"),
                HeaderValue::from_static("bytes"),
            );
        }
    }

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        let stream = upstream.bytes_stream();
        Body::from_stream(stream)
    };

    let mut response = (status, body).into_response();
    response.headers_mut().extend(out_headers);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::{
        is_retrievable_content_key, parse_range, sniff_content_type, DEFAULT_CONTENT_TYPE,
    };

    #[test]
    fn sniff_mime_from_magic_bytes() {
        let png: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff_content_type(png), "image/png");

        let jpeg: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        assert_eq!(sniff_content_type(jpeg), "image/jpeg");

        assert_eq!(
            sniff_content_type(b"just some plain text, not a known file format\n"),
            DEFAULT_CONTENT_TYPE
        );
        assert_eq!(sniff_content_type(br#"{"a":1}"#), DEFAULT_CONTENT_TYPE);
        assert_eq!(sniff_content_type(&[]), DEFAULT_CONTENT_TYPE);
    }

    #[test]
    fn retrievable_content_keys() {
        assert!(is_retrievable_content_key(
            "bafkreiahsvnr4x4rnskhkwfbnbplkbqhzb3xagdwpyfy44lgcndmhyizde"
        ));
        assert!(is_retrievable_content_key(&"a".repeat(64)));
        assert!(is_retrievable_content_key(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_retrievable_content_key(&"A".repeat(64)));
        assert!(!is_retrievable_content_key(&"a".repeat(63)));
        assert!(!is_retrievable_content_key(&"g".repeat(64)));
        assert!(!is_retrievable_content_key("../etc/passwd"));
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some(Some((0, 99))));
        assert_eq!(parse_range("bytes=100-", 1000), Some(Some((100, 999))));
        assert_eq!(parse_range("bytes=-100", 1000), Some(Some((900, 999))));
        assert_eq!(parse_range("bytes=500-9999", 1000), Some(Some((500, 999))));
        assert_eq!(parse_range("bytes=1000-2000", 1000), Some(None));
        assert_eq!(parse_range("bytes=-0", 1000), Some(None));
        assert_eq!(parse_range("bytes=50-10", 1000), Some(None));
        assert_eq!(parse_range("bytes=0-10,20-30", 1000), None);
        assert_eq!(parse_range("items=0-10", 1000), None);
        assert_eq!(parse_range("bytes=-", 1000), None);
    }
}
