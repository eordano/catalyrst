use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

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

/// Builder stores predate the canonical-CID rule and still hold merely-alphanumeric keys
/// (shortened digests, uppercase hex); the length window is only a path guard.
fn is_legacy_storage_key(hash: &str) -> bool {
    (32..=128).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn is_valid_content_hash(hash: &str) -> bool {
    catalyrst_hashing::is_canonical_cid(hash) || is_legacy_storage_key(hash)
}

struct ExistsCache {
    map: HashMap<String, (Instant, bool)>,
    order: VecDeque<(String, Instant)>,
    inflight: HashMap<String, Arc<tokio::sync::OnceCell<bool>>>,
}

impl ExistsCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            inflight: HashMap::new(),
        }
    }

    fn fresh(&self, key: &str) -> Option<bool> {
        self.map
            .get(key)
            .filter(|(at, _)| at.elapsed() < EXISTS_TTL)
            .map(|(_, ok)| *ok)
    }

    /// Entries are never refreshed in place, so insertion order is expiry order.
    fn insert(&mut self, key: String, ok: bool) {
        let now = Instant::now();
        while self.map.len() >= EXISTS_CACHE_MAX && !self.map.contains_key(&key) {
            let Some((old_key, old_at)) = self.order.pop_front() else {
                break;
            };
            if self.map.get(&old_key).is_some_and(|(at, _)| *at == old_at) {
                self.map.remove(&old_key);
            }
        }
        if self.order.len() >= 2 * EXISTS_CACHE_MAX {
            let map = &self.map;
            self.order
                .retain(|(k, at)| map.get(k).is_some_and(|(a, _)| a == at));
        }
        self.map.insert(key.clone(), (now, ok));
        self.order.push_back((key, now));
    }
}

fn exists_cache() -> &'static std::sync::Mutex<ExistsCache> {
    static C: std::sync::OnceLock<std::sync::Mutex<ExistsCache>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(ExistsCache::new()))
}

fn exists_status(ok: bool) -> Response {
    if ok {
        StatusCode::OK.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub async fn head_storage_content_exists(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(params): Query<ContentParams>,
) -> Response {
    if !is_valid_content_hash(&hash) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let cell = match exists_cache().lock() {
        Ok(mut cache) => {
            if let Some(ok) = cache.fresh(&hash) {
                return exists_status(ok);
            }
            Some(cache.inflight.entry(hash.clone()).or_default().clone())
        }
        Err(_) => None,
    };

    let target = content_url(&state, &hash, &params);
    let probe = || async {
        matches!(state.http.head(&target).send().await, Ok(r) if r.status().is_success())
    };
    let ok = match &cell {
        Some(cell) => *cell.get_or_init(probe).await,
        None => probe().await,
    };
    if let Ok(mut cache) = exists_cache().lock() {
        if let Some(cell) = &cell {
            if cache
                .inflight
                .get(&hash)
                .is_some_and(|c| Arc::ptr_eq(c, cell))
            {
                cache.inflight.remove(&hash);
            }
        }
        cache.insert(hash, ok);
    }
    exists_status(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exists_cache_evicts_oldest_first_and_stays_bounded() {
        let mut cache = ExistsCache::new();
        for i in 0..EXISTS_CACHE_MAX {
            cache.insert(format!("k{i}"), i % 2 == 0);
        }
        cache.insert("k0".to_string(), false);
        assert_eq!(cache.map.len(), EXISTS_CACHE_MAX);
        assert_eq!(cache.fresh("k0"), Some(false));
        cache.insert("new".to_string(), true);
        assert_eq!(cache.map.len(), EXISTS_CACHE_MAX);
        assert_eq!(cache.fresh("k1"), None, "oldest insert goes first");
        assert_eq!(cache.fresh("k0"), Some(false), "re-inserted key kept");
        assert_eq!(cache.fresh("new"), Some(true));
        assert!(cache.order.len() <= 2 * EXISTS_CACHE_MAX);
    }

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
