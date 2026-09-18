use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, Context, Result};
use axum::body::Bytes;
use reqwest::Client;
use serde_json::{json, Value};

use crate::pull_cache::{write_atomic, PullCache};

pub const CATALOG_FILE: &str = "catalog.json";
pub const PULLED_DIR: &str = "pulled";

const MISS_TTL: Duration = Duration::from_secs(300);
const CATALOG_RECHECK: Duration = Duration::from_secs(30);
const MISS_CACHE_MAX: usize = 65536;
const MAX_CONTENT_BYTES: usize = 256 * 1024 * 1024;

pub struct PullThrough {
    pub bucket: String,
    pub budget: u64,
    pub http: Client,
}

struct PullSource {
    bucket: String,
    http: Client,
    cache: PullCache,
}

pub struct CatalogStore {
    dir: PathBuf,
    pull: Option<PullSource>,
    catalog: Mutex<Option<(Instant, Arc<CachedCatalog>)>>,
    misses: Mutex<HashMap<String, Instant>>,
    pulling: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// Per-hash pull gate; the map entry goes away with the last holder.
struct PullGate<'a> {
    store: &'a CatalogStore,
    hash: String,
    gate: Arc<tokio::sync::Mutex<()>>,
}

impl Drop for PullGate<'_> {
    fn drop(&mut self) {
        if let Ok(mut m) = self.store.pulling.lock() {
            if Arc::strong_count(&self.gate) <= 2
                && m.get(&self.hash)
                    .is_some_and(|g| Arc::ptr_eq(g, &self.gate))
            {
                m.remove(&self.hash);
            }
        }
    }
}

#[derive(Clone)]
struct CachedCatalog {
    modified: SystemTime,
    len: u64,
    body: Bytes,
    names: HashMap<String, String>,
}

pub struct Content {
    pub bytes: Bytes,
    pub content_type: &'static str,
}

#[derive(Debug)]
pub enum UpstreamError {
    Status(u16),
    Unreachable(anyhow::Error),
    Rejected(anyhow::Error),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status(s) => write!(f, "upstream answered {s}"),
            Self::Unreachable(e) => write!(f, "upstream unreachable: {e:#}"),
            Self::Rejected(e) => write!(f, "upstream response rejected: {e:#}"),
        }
    }
}

#[derive(Debug)]
pub enum ContentError {
    InvalidHash,
    NotFound,
    Upstream(UpstreamError),
    Io(anyhow::Error),
}

pub fn content_type_for_name(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "json" => "application/json",
        "js" | "mjs" => "text/javascript",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "ktx2" => "image/ktx2",
        "bin" | "crdt" => "application/octet-stream",
        _ => return None,
    })
}

pub fn sniff_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.starts_with(b"glTF") {
        "model/gltf-binary"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{' || *b == b'[')
    {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

fn packs_of(raw: Value) -> Result<Vec<Value>> {
    let packs = match raw {
        Value::Array(a) => a,
        Value::Object(mut o) => o
            .remove("data")
            .or_else(|| o.remove("assetPacks"))
            .and_then(|v| v.as_array().cloned())
            .ok_or_else(|| anyhow!("{CATALOG_FILE} has neither a data nor an assetPacks array"))?,
        _ => return Err(anyhow!("{CATALOG_FILE} is not a JSON object")),
    };
    Ok(packs)
}

fn index_names(packs: &[Value]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for pack in packs {
        if let Some(h) = pack.get("thumbnail").and_then(Value::as_str) {
            names.insert(h.to_string(), "thumbnail.png".to_string());
        }
        let Some(assets) = pack.get("assets").and_then(Value::as_array) else {
            continue;
        };
        for asset in assets {
            let Some(contents) = asset.get("contents").and_then(Value::as_object) else {
                continue;
            };
            for (file, hash) in contents {
                if let Some(h) = hash.as_str() {
                    names.insert(h.to_string(), file.clone());
                }
            }
        }
    }
    names
}

pub async fn fetch_verified(
    http: &Client,
    url: &str,
    hash: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, UpstreamError> {
    let resp = http.get(url).send().await.map_err(|e| {
        UpstreamError::Unreachable(anyhow::Error::from(e).context(format!("GET {url}")))
    })?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(UpstreamError::Status(status.as_u16()));
    }
    if resp.content_length().is_some_and(|n| n > max_bytes as u64) {
        return Err(UpstreamError::Rejected(anyhow!(
            "{url} exceeds {max_bytes} bytes"
        )));
    }
    let bytes = resp.bytes().await.map_err(|e| {
        UpstreamError::Unreachable(anyhow::Error::from(e).context(format!("read {url}")))
    })?;
    if bytes.len() > max_bytes {
        return Err(UpstreamError::Rejected(anyhow!(
            "{url} exceeds {max_bytes} bytes"
        )));
    }
    if !bytes_match_cid(&bytes, hash) {
        return Err(UpstreamError::Rejected(anyhow!(
            "{url} served bytes whose CID is not {hash}"
        )));
    }
    Ok(Some(bytes.to_vec()))
}

pub fn bytes_match_cid(bytes: &[u8], hash: &str) -> bool {
    catalyrst_hashing::verify_hash(bytes, hash)
        || (hash.starts_with("Qm") && catalyrst_hashing::hash_bytes_v0_unixfs(bytes) == hash)
}

impl CatalogStore {
    pub fn new(dir: impl Into<PathBuf>, pull: Option<PullThrough>) -> Result<Self> {
        let dir = dir.into();
        let pull = match pull {
            Some(p) => {
                let bucket = p.bucket.trim().trim_end_matches('/').to_string();
                if bucket.is_empty() {
                    return Err(anyhow!("pull-through needs a content bucket URL"));
                }
                let cache = PullCache::open(dir.join(PULLED_DIR), p.budget)
                    .context("open builder pull cache")?;
                Some(PullSource {
                    bucket,
                    http: p.http,
                    cache,
                })
            }
            None => None,
        };
        Ok(Self {
            dir,
            pull,
            catalog: Mutex::new(None),
            misses: Mutex::new(HashMap::new()),
            pulling: Mutex::new(HashMap::new()),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn pull_cache(&self) -> Option<&PullCache> {
        self.pull.as_ref().map(|p| &p.cache)
    }

    fn remember_catalog(&self, catalog: Option<Arc<CachedCatalog>>) {
        if let Ok(mut slot) = self.catalog.lock() {
            *slot = catalog.map(|c| (Instant::now(), c));
        }
    }

    async fn load_catalog(&self) -> Result<Option<Arc<CachedCatalog>>> {
        let cached = self.catalog.lock().ok().and_then(|c| c.clone());
        if let Some((checked, catalog)) = &cached {
            if checked.elapsed() < CATALOG_RECHECK {
                return Ok(Some(catalog.clone()));
            }
        }
        let path = self.dir.join(CATALOG_FILE);
        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.remember_catalog(None);
                return Ok(None);
            }
            Err(e) => return Err(e).with_context(|| format!("stat {}", path.display())),
        };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let len = meta.len();
        if let Some((_, catalog)) = cached {
            if catalog.modified == modified && catalog.len == len {
                self.remember_catalog(Some(catalog.clone()));
                return Ok(Some(catalog));
            }
        }
        let raw = tokio::fs::read(&path)
            .await
            .with_context(|| format!("read {}", path.display()))?;
        let value: Value =
            serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()))?;
        let packs = packs_of(value)?;
        let names = index_names(&packs);
        let body = Bytes::from(serde_json::to_vec(&json!({ "ok": true, "data": packs }))?);
        let cached = Arc::new(CachedCatalog {
            modified,
            len,
            body,
            names,
        });
        self.remember_catalog(Some(cached.clone()));
        Ok(Some(cached))
    }

    pub async fn asset_packs(&self) -> Result<Option<Bytes>> {
        Ok(self.load_catalog().await?.map(|c| c.body.clone()))
    }

    fn pull_gate(&self, hash: &str) -> PullGate<'_> {
        let gate = self
            .pulling
            .lock()
            .ok()
            .map(|mut m| m.entry(hash.to_string()).or_default().clone())
            .unwrap_or_default();
        PullGate {
            store: self,
            hash: hash.to_string(),
            gate,
        }
    }

    async fn content_type(&self, hash: &str, bytes: &[u8]) -> &'static str {
        let named = match self.load_catalog().await {
            Ok(Some(c)) => c.names.get(hash).and_then(|n| content_type_for_name(n)),
            _ => None,
        };
        named.unwrap_or_else(|| sniff_content_type(bytes))
    }

    fn recently_missed(&self, hash: &str) -> bool {
        self.misses
            .lock()
            .ok()
            .and_then(|m| m.get(hash).copied())
            .is_some_and(|at| at.elapsed() < MISS_TTL)
    }

    fn record_miss(&self, hash: &str) {
        if let Ok(mut m) = self.misses.lock() {
            if m.len() >= MISS_CACHE_MAX {
                m.retain(|_, at| at.elapsed() < MISS_TTL);
            }
            if m.len() < MISS_CACHE_MAX {
                m.insert(hash.to_string(), Instant::now());
            }
        }
    }

    async fn served(&self, hash: &str, bytes: Vec<u8>) -> Content {
        let content_type = self.content_type(hash, &bytes).await;
        Content {
            bytes: Bytes::from(bytes),
            content_type,
        }
    }

    pub async fn content(&self, hash: &str) -> Result<Content, ContentError> {
        if !catalyrst_hashing::is_canonical_cid(hash) {
            return Err(ContentError::InvalidHash);
        }
        let path = self.dir.join(hash);
        match tokio::fs::read(&path).await {
            Ok(bytes) => return Ok(self.served(hash, bytes).await),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(ContentError::Io(
                    anyhow::Error::from(e).context(format!("read {}", path.display())),
                ))
            }
        }
        let Some(pull) = self.pull.as_ref() else {
            return Err(ContentError::NotFound);
        };
        match pull.cache.read(hash).await {
            Ok(Some(bytes)) => return Ok(self.served(hash, bytes).await),
            Ok(None) => {}
            Err(e) => {
                return Err(ContentError::Io(
                    anyhow::Error::from(e).context(format!("read pulled {hash}")),
                ))
            }
        }
        if self.recently_missed(hash) {
            return Err(ContentError::NotFound);
        }
        let gate = self.pull_gate(hash);
        let _flight = match gate.gate.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                let guard = gate.gate.lock().await;
                // A follower re-reads whatever the flight ahead of it stored or missed.
                match pull.cache.read(hash).await {
                    Ok(Some(bytes)) => return Ok(self.served(hash, bytes).await),
                    Ok(None) => {}
                    Err(e) => {
                        return Err(ContentError::Io(
                            anyhow::Error::from(e).context(format!("read pulled {hash}")),
                        ))
                    }
                }
                if self.recently_missed(hash) {
                    return Err(ContentError::NotFound);
                }
                guard
            }
        };
        let url = format!("{}/contents/{hash}", pull.bucket);
        let max_bytes =
            MAX_CONTENT_BYTES.min(usize::try_from(pull.cache.budget()).unwrap_or(usize::MAX));
        let bytes = match fetch_verified(&pull.http, &url, hash, max_bytes).await {
            Ok(Some(b)) => b,
            Ok(None) => {
                self.record_miss(hash);
                return Err(ContentError::NotFound);
            }
            Err(e) => return Err(ContentError::Upstream(e)),
        };
        pull.cache
            .store(hash, &bytes)
            .await
            .map_err(ContentError::Io)?;
        Ok(self.served(hash, bytes).await)
    }
}

pub async fn seed(dir: &Path, base_url: &str, hashes: &[String]) -> Result<usize> {
    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("build http client")?;
    let base = base_url.trim_end_matches('/');
    let mut stored = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for hash in hashes {
        if !catalyrst_hashing::is_canonical_cid(hash) {
            failed.push(format!("{hash}: not a canonical CID"));
            continue;
        }
        if dir.join(hash).exists() {
            continue;
        }
        match fetch_verified(&http, &format!("{base}/{hash}"), hash, MAX_CONTENT_BYTES).await {
            Ok(Some(bytes)) => {
                write_atomic(dir, hash, &bytes).await?;
                stored += 1;
            }
            Ok(None) => failed.push(format!("{hash}: not served by {base}")),
            Err(e) => failed.push(format!("{hash}: {e}")),
        }
    }
    println!(
        "seed: {} fetched, {} already present, {} failed -> {}",
        stored,
        hashes.len() - stored - failed.len(),
        failed.len(),
        dir.display()
    );
    if failed.is_empty() {
        Ok(stored)
    } else {
        Err(anyhow!("seed failed for:\n  {}", failed.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Path as AxumPath;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("catalyrst-builder-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const GLB: &[u8] = b"glTF\x02\x00\x00\x00model-bytes";

    fn local(dir: impl Into<PathBuf>) -> CatalogStore {
        CatalogStore::new(dir, None).unwrap()
    }

    fn pulling(dir: impl Into<PathBuf>, bucket: String, budget: u64) -> CatalogStore {
        CatalogStore::new(
            dir,
            Some(PullThrough {
                bucket,
                budget,
                http: Client::new(),
            }),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn asset_packs_is_none_until_the_catalog_is_built() {
        let store = local(scratch());
        assert!(store.asset_packs().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn asset_packs_wraps_the_built_catalog_in_the_builder_server_envelope() {
        let dir = scratch();
        let glb_hash = catalyrst_hashing::hash_bytes_v1(GLB);
        std::fs::write(
            dir.join(CATALOG_FILE),
            json!({ "data": [{ "id": "p1", "title": "Pack", "assets": [{ "id": "a1", "name": "A",
                "model": "m/a.glb", "contents": { "m/a.glb": glb_hash } }] }] })
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.join(&glb_hash), GLB).unwrap();
        let store = local(&dir);

        let body = store.asset_packs().await.unwrap().unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["data"][0]["title"], json!("Pack"));
        assert_eq!(
            v["data"][0]["assets"][0]["contents"]["m/a.glb"],
            json!(glb_hash)
        );

        let content = store.content(&glb_hash).await.unwrap();
        assert_eq!(content.content_type, "model/gltf-binary");
        assert_eq!(&content.bytes[..], GLB);
    }

    #[tokio::test]
    async fn asset_packs_accepts_the_npm_catalog_layout() {
        let dir = scratch();
        std::fs::write(
            dir.join(CATALOG_FILE),
            json!({ "assetPacks": [{ "id": "p1", "name": "Pack", "assets": [] }] }).to_string(),
        )
        .unwrap();
        let store = local(&dir);
        let v: Value =
            serde_json::from_slice(&store.asset_packs().await.unwrap().unwrap()).unwrap();
        assert_eq!(v["data"][0]["name"], json!("Pack"));
    }

    #[tokio::test]
    async fn content_rejects_non_cids_and_misses_without_pull_through() {
        let store = local(scratch());
        assert!(matches!(
            store.content("../../etc/passwd").await,
            Err(ContentError::InvalidHash)
        ));
        assert!(matches!(
            store.content("hello").await,
            Err(ContentError::InvalidHash)
        ));
        let missing = catalyrst_hashing::hash_bytes_v1(b"absent");
        assert!(matches!(
            store.content(&missing).await,
            Err(ContentError::NotFound)
        ));
    }

    #[tokio::test]
    async fn content_sniffs_types_for_files_outside_the_catalog() {
        let dir = scratch();
        let png = b"\x89PNG\r\n\x1a\nrest";
        let hash = catalyrst_hashing::hash_bytes(png);
        std::fs::write(dir.join(&hash), png).unwrap();
        let store = local(&dir);
        assert_eq!(
            store.content(&hash).await.unwrap().content_type,
            "image/png"
        );
    }

    async fn bucket(hits: Arc<AtomicUsize>, status: StatusCode, served: &'static [u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/contents/{hash}",
            get(move |AxumPath(_): AxumPath<String>| {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    (status, served.to_vec())
                }
            }),
        );
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn content_pulls_through_into_the_pulled_dir_and_serves_from_it_afterwards() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = bucket(hits.clone(), StatusCode::OK, GLB).await;
        let store = pulling(&dir, base, 1 << 20);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);

        let first = store.content(&hash).await.unwrap();
        assert_eq!(&first.bytes[..], GLB);
        assert_eq!(first.content_type, "model/gltf-binary");
        assert!(!dir.join(&hash).exists());
        assert!(dir.join(PULLED_DIR).join(&hash).exists());

        let second = store.content(&hash).await.unwrap();
        assert_eq!(&second.bytes[..], GLB);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(store.pull_cache().unwrap().total_bytes(), GLB.len() as u64);
    }

    #[tokio::test]
    async fn content_without_pull_through_never_contacts_the_bucket() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let _base = bucket(hits.clone(), StatusCode::OK, GLB).await;
        let store = local(&dir);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);
        assert!(matches!(
            store.content(&hash).await,
            Err(ContentError::NotFound)
        ));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(store.pull_cache().is_none());
    }

    #[tokio::test]
    async fn content_refuses_bytes_that_do_not_hash_to_the_requested_cid_without_caching_the_failure(
    ) {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = bucket(hits.clone(), StatusCode::OK, GLB).await;
        let store = pulling(&dir, base, 1 << 20);
        let other = catalyrst_hashing::hash_bytes_v1(b"something else");

        for _ in 0..2 {
            assert!(matches!(
                store.content(&other).await,
                Err(ContentError::Upstream(UpstreamError::Rejected(_)))
            ));
        }
        assert!(!dir.join(PULLED_DIR).join(&other).exists());
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn upstream_404_is_a_cached_miss() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = bucket(hits.clone(), StatusCode::NOT_FOUND, b"").await;
        let store = pulling(&dir, base, 1 << 20);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);

        for _ in 0..3 {
            assert!(matches!(
                store.content(&hash).await,
                Err(ContentError::NotFound)
            ));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn upstream_5xx_surfaces_as_an_upstream_error_and_is_not_cached() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = bucket(hits.clone(), StatusCode::SERVICE_UNAVAILABLE, b"down").await;
        let store = pulling(&dir, base, 1 << 20);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);

        for _ in 0..2 {
            assert!(matches!(
                store.content(&hash).await,
                Err(ContentError::Upstream(UpstreamError::Status(503)))
            ));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(!dir.join(PULLED_DIR).join(&hash).exists());
    }

    #[tokio::test]
    async fn unreachable_bucket_surfaces_as_unreachable_and_is_not_cached() {
        let dir = scratch();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let store = pulling(&dir, dead, 1 << 20);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);

        assert!(matches!(
            store.content(&hash).await,
            Err(ContentError::Upstream(UpstreamError::Unreachable(_)))
        ));
        assert!(!store.recently_missed(&hash));
    }

    #[tokio::test]
    async fn pulled_content_is_bounded_by_the_budget() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = bucket(hits.clone(), StatusCode::OK, GLB).await;
        let store = pulling(&dir, base, GLB.len() as u64 - 1);
        let hash = catalyrst_hashing::hash_bytes_v1(GLB);

        assert!(matches!(
            store.content(&hash).await,
            Err(ContentError::Upstream(UpstreamError::Rejected(_)))
        ));
        assert!(!dir.join(PULLED_DIR).join(&hash).exists());
        assert_eq!(store.pull_cache().unwrap().total_bytes(), 0);
    }

    #[test]
    fn legacy_builder_qm_hashes_are_ipfs_unixfs_not_catalyst_raw_digests() {
        let glb = b"glTF legacy builder model";
        let catalyst = catalyrst_hashing::hash_bytes(glb);
        let ipfs = catalyrst_hashing::hash_bytes_v0_unixfs(glb);
        assert_ne!(catalyst, ipfs);
        assert!(bytes_match_cid(glb, &catalyst));
        assert!(bytes_match_cid(glb, &ipfs));
        assert!(bytes_match_cid(glb, &catalyrst_hashing::hash_bytes_v1(glb)));
        assert!(!bytes_match_cid(b"other bytes", &ipfs));
    }

    #[tokio::test]
    async fn seed_stores_verified_bytes_and_reports_liars() {
        let dir = scratch();
        let hits = Arc::new(AtomicUsize::new(0));
        let base = format!("{}contents", bucket(hits, StatusCode::OK, GLB).await);
        let good = catalyrst_hashing::hash_bytes_v1(GLB);
        let bad = catalyrst_hashing::hash_bytes(b"not the glb");

        assert_eq!(
            seed(&dir, &base, std::slice::from_ref(&good))
                .await
                .unwrap(),
            1
        );
        assert_eq!(std::fs::read(dir.join(&good)).unwrap(), GLB);
        assert_eq!(
            seed(&dir, &base, std::slice::from_ref(&good))
                .await
                .unwrap(),
            0
        );

        let err = seed(&dir, &base, std::slice::from_ref(&bad))
            .await
            .unwrap_err();
        assert!(err.to_string().contains(&bad));
        assert!(!dir.join(&bad).exists());
    }
}
