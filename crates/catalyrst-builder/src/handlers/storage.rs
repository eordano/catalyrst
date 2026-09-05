use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ContentParams {
    pub ts: Option<String>,
}

fn content_url(state: &AppState, hash: &str, params: &ContentParams) -> String {
    let bucket = state.content_bucket_url.trim_end_matches('/');
    let mut target = format!("{}/contents/{}", bucket, hash);
    if let Some(ts) = params.ts.as_deref().filter(|s| !s.is_empty()) {
        target.push_str("?ts=");
        target.push_str(ts);
    }
    target
}

pub async fn get_storage_content(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    let target = content_url(&state, &hash, &params);

    let mut resp = (StatusCode::MOVED_PERMANENTLY, ()).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&target) {
        headers.insert(header::LOCATION, v);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public,max-age=31536000,immutable"),
    );
    resp
}

const EXISTS_TTL: std::time::Duration = std::time::Duration::from_secs(300);
const EXISTS_CACHE_MAX: usize = 8192;

/// Builder stores predate the canonical-CID rule and still hold keys that are merely
/// alphanumeric -- shortened digests, uppercase hex. The window is what keeps a key from
/// being a path, so it stays as wide as the deployed stores are.
fn is_legacy_storage_key(hash: &str) -> bool {
    (32..=128).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn is_valid_content_hash(hash: &str) -> bool {
    catalyrst_hashing::is_canonical_cid(hash) || is_legacy_storage_key(hash)
}

fn exists_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>> {
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>>,
    > = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn exists_cache_insert(
    map: &mut std::collections::HashMap<String, (std::time::Instant, bool)>,
    key: String,
    val: (std::time::Instant, bool),
) {
    if map.len() >= EXISTS_CACHE_MAX && !map.contains_key(&key) {
        map.retain(|_, (at, _)| at.elapsed() < EXISTS_TTL);
        while map.len() >= EXISTS_CACHE_MAX {
            let oldest = map
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    map.remove(&k);
                }
                None => break,
            }
        }
    }
    map.insert(key, val);
}

pub async fn head_storage_content_exists(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    if !is_valid_content_hash(&hash) {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some((at, ok)) = exists_cache()
        .lock()
        .ok()
        .and_then(|m| m.get(&hash).copied())
    {
        if at.elapsed() < EXISTS_TTL {
            return if ok {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
            .into_response();
        }
    }

    let target = content_url(&state, &hash, &params);
    let ok = matches!(state.http.head(&target).send().await, Ok(r) if r.status().is_success());
    if let Ok(mut m) = exists_cache().lock() {
        exists_cache_insert(&mut m, hash, (std::time::Instant::now(), ok));
    }
    if ok {
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_cidv0() {
        assert!(is_valid_content_hash(
            "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"
        ));
    }

    #[test]
    fn accepts_canonical_cidv1() {
        assert!(is_valid_content_hash(
            "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenora7777"
        ));
    }

    #[test]
    fn accepts_legacy_sha256_hex() {
        assert!(is_valid_content_hash(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
        ));
    }

    #[test]
    fn accepts_legacy_uppercase_and_short_keys_deployed_stores_still_hold() {
        assert!(is_valid_content_hash(
            "A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2"
        ));
        assert!(is_valid_content_hash(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1"
        ));
        assert!(is_valid_content_hash(&"a".repeat(40)));
    }

    #[test]
    fn rejects_keys_outside_the_legacy_length_window() {
        assert!(!is_valid_content_hash(&"a".repeat(31)));
        assert!(!is_valid_content_hash(&"a".repeat(129)));
    }

    #[test]
    fn rejects_non_alphanumeric_keys() {
        assert!(!is_valid_content_hash(&format!("{}/x", "a".repeat(40))));
        assert!(!is_valid_content_hash(&format!("{}.x", "a".repeat(40))));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_content_hash("../../../etc/passwd"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_valid_content_hash(""));
    }

    #[test]
    fn rejects_arbitrary_string() {
        assert!(!is_valid_content_hash("hello world"));
    }
}
