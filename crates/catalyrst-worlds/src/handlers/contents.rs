use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::http::ApiError;
use crate::AppState;

const MAX_AVAILABLE_CONTENT_CIDS: usize = 500;
const AVAILABLE_CONTENT_CONCURRENCY: usize = 32;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct AvailableContentEntry {
    pub cid: String,
    pub available: bool,
}

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

/// Hash-addressed blobs never change, so (len, mime) per hash is memoized for the
/// process lifetime; a vanished file (GC) drops the entry on the next open.
#[derive(Clone, Copy)]
struct LocalMeta {
    len: u64,
    mime: &'static str,
}

const LOCAL_META_CAPACITY: u64 = 65_536;
const UPSTREAM_MISS_TTL: std::time::Duration = std::time::Duration::from_secs(30);

fn local_meta_cache() -> &'static moka::future::Cache<String, LocalMeta> {
    static CACHE: std::sync::OnceLock<moka::future::Cache<String, LocalMeta>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        moka::future::Cache::builder()
            .max_capacity(LOCAL_META_CAPACITY)
            .build()
    })
}

fn upstream_miss_cache() -> &'static moka::future::Cache<String, ()> {
    static CACHE: std::sync::OnceLock<moka::future::Cache<String, ()>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        moka::future::Cache::builder()
            .max_capacity(LOCAL_META_CAPACITY)
            .time_to_live(UPSTREAM_MISS_TTL)
            .build()
    })
}

/// Reads the sniff window from an open handle; the caller repositions it.
async fn sniff_open_file(file: &mut tokio::fs::File) -> Option<&'static str> {
    let mut head = Vec::with_capacity(MIME_SNIFF_BYTES as usize);
    file.take(MIME_SNIFF_BYTES)
        .read_to_end(&mut head)
        .await
        .ok()?;
    Some(sniff_content_type(&head))
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

#[utoipa::path(
    get,
    path = "/contents/{hash}",
    tag = "contents",
    params(("hash" = String, Path)),
    responses(
        (status = 200, content_type = "application/octet-stream"),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::GET).await
}

#[utoipa::path(
    head,
    path = "/contents/{hash}",
    tag = "contents",
    params(("hash" = String, Path)),
    responses(
        (status = 200),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn head_content(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::HEAD).await
}

#[utoipa::path(
    get,
    path = "/ipfs/{hash}",
    tag = "contents",
    params(("hash" = String, Path)),
    responses(
        (status = 200, content_type = "application/octet-stream"),
        (status = 400),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn get_ipfs(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::GET).await
}

#[utoipa::path(
    head,
    path = "/ipfs/{hash}",
    tag = "contents",
    params(("hash" = String, Path)),
    responses(
        (status = 200),
        (status = 400),
        (status = 404, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn head_ipfs(
    state: State<AppState>,
    hash: Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    proxy(state, hash, headers, Method::HEAD).await
}

#[utoipa::path(
    get,
    path = "/available-content",
    tag = "contents",
    params(("cid" = Option<Vec<String>>, Query)),
    responses(
        (status = 200, body = Vec<AvailableContentEntry>),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn available_content(
    State(state): State<AppState>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> Result<axum::Json<Vec<AvailableContentEntry>>, ApiError> {
    let cids: Vec<&str> = query
        .as_deref()
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.strip_prefix("cid="))
        .filter(|c| !c.is_empty())
        .collect();

    if cids.is_empty() {
        return Err(ApiError::bad_request(
            "At least one cid query parameter is required.",
        ));
    }
    if cids.len() > MAX_AVAILABLE_CONTENT_CIDS {
        return Err(ApiError::bad_request(format!(
            "Too many cids requested; the maximum allowed is {MAX_AVAILABLE_CONTENT_CIDS}."
        )));
    }
    for cid in &cids {
        if !is_ipfs_v2(cid) {
            return Err(ApiError::bad_request(format!("Invalid cid: {cid}")));
        }
    }

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(AVAILABLE_CONTENT_CONCURRENCY));
    let mut set = tokio::task::JoinSet::new();
    for (i, cid) in cids.iter().enumerate() {
        let path = state.cfg.contents_dir.join(cid);
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await;
            let available = tokio::fs::metadata(path)
                .await
                .map(|m| m.is_file())
                .unwrap_or(false);
            (i, available)
        });
    }
    let mut available = vec![false; cids.len()];
    while let Some(res) = set.join_next().await {
        let (i, ok) = res.map_err(|e| ApiError::internal(format!("available-content: {e}")))?;
        available[i] = ok;
    }
    let out = cids
        .into_iter()
        .zip(available)
        .map(|(cid, available)| AvailableContentEntry {
            cid: cid.to_string(),
            available,
        })
        .collect();
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
    let memo = local_meta_cache().get(&hash).await;
    match tokio::fs::File::open(&local).await {
        Ok(mut file) => {
            let meta = match memo {
                Some(m) => Some(m),
                None => match file.metadata().await {
                    Ok(md) if md.is_file() => {
                        let sniffed = sniff_open_file(&mut file).await;
                        file.seek(std::io::SeekFrom::Start(0))
                            .await
                            .map_err(|e| ApiError::internal(format!("local content seek: {e}")))?;
                        let m = LocalMeta {
                            len: md.len(),
                            mime: sniffed.unwrap_or(DEFAULT_CONTENT_TYPE),
                        };
                        if sniffed.is_some() {
                            local_meta_cache().insert(hash.clone(), m).await;
                        }
                        Some(m)
                    }
                    _ => None,
                },
            };
            if let Some(meta) = meta {
                return serve_local(file, meta, &hash, &headers, method).await;
            }
        }
        Err(_) => {
            if memo.is_some() {
                local_meta_cache().invalidate(&hash).await;
            }
        }
    }

    let Some(upstream_base) = state.cfg.contents_upstream_url.as_deref() else {
        return Err(ApiError::not_found(format!(
            "content {hash} is not stored locally and CONTENTS_UPSTREAM_URL is unset, \
             so there is no upstream to read through to"
        )));
    };

    if upstream_miss_cache().contains_key(&hash) {
        return Err(ApiError::not_found(format!(
            "content {hash} is not stored locally and the upstream reported it missing"
        )));
    }

    let url = format!("{upstream_base}/contents/{hash}");

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
    if status == StatusCode::NOT_FOUND {
        upstream_miss_cache().insert(hash.clone(), ()).await;
    }

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

async fn serve_local(
    mut file: tokio::fs::File,
    meta: LocalMeta,
    hash: &str,
    headers: &HeaderMap,
    method: Method,
) -> Result<Response, ApiError> {
    let size = meta.len;
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|r| parse_range(r, size));
    match range {
        Some(None) => Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("content-range", format!("bytes */{size}"))
            .header("accept-ranges", "bytes")
            .body(Body::empty())
            .unwrap()),
        Some(Some((start, end))) => {
            let builder = Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header("content-type", meta.mime)
                .header("content-range", format!("bytes {start}-{end}/{size}"))
                .header("content-length", end - start + 1)
                .header("etag", format!("\"{hash}\""))
                .header("cache-control", IMMUTABLE_CACHE_CONTROL)
                .header("access-control-expose-headers", EXPOSED_HEADERS)
                .header("accept-ranges", "bytes");
            if method == Method::HEAD {
                return Ok(builder.body(Body::empty()).unwrap());
            }
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| ApiError::internal(format!("local content seek: {e}")))?;
            let stream = ReaderStream::new(file.take(end - start + 1));
            Ok(builder.body(Body::from_stream(stream)).unwrap())
        }
        None => {
            let builder = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", meta.mime)
                .header("content-length", size)
                .header("etag", format!("\"{hash}\""))
                .header("cache-control", IMMUTABLE_CACHE_CONTROL)
                .header("access-control-expose-headers", EXPOSED_HEADERS)
                .header("accept-ranges", "bytes");
            if method == Method::HEAD {
                return Ok(builder.body(Body::empty()).unwrap());
            }
            Ok(builder
                .body(Body::from_stream(ReaderStream::new(file)))
                .unwrap())
        }
    }
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
