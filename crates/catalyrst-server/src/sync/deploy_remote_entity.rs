use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use futures::StreamExt;
use reqwest::Client;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use super::backends::LiveSyncDeployer;
use super::content_encoding::{decode_content_encoding, response_content_encoding};
use super::{AuthChain, DeploymentContext, SyncError};

const MAX_DOWNLOAD_RETRIES: u32 = 3;
const RETRY_WAIT_MS: u64 = 500;

const MAX_BODY_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachedEntityVerification {
    Match,
    Mismatch,
    Unverifiable,
}

fn verify_cached_entity(bytes: &[u8], entity_id: &str) -> CachedEntityVerification {
    let v0_shape = entity_id.len() == 46 && entity_id.starts_with("Qm");
    let v1_shape = entity_id.len() == 59 && entity_id.starts_with("ba");
    if !(v0_shape || v1_shape) || !catalyrst_hashing::is_canonical_cid(entity_id) {
        return CachedEntityVerification::Unverifiable;
    }
    if catalyrst_hashing::verify_hash(bytes, entity_id) {
        CachedEntityVerification::Match
    } else {
        CachedEntityVerification::Mismatch
    }
}

fn eviction_locks() -> &'static parking_lot::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static LOCKS: OnceLock<parking_lot::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    LOCKS.get_or_init(Default::default)
}

async fn evict_corrupt_entity_file(
    storage: &catalyrst_storage::ContentStorage,
    entity_id: &str,
) -> &'static str {
    let lock = {
        let mut map = eviction_locks().lock();
        map.entry(entity_id.to_string()).or_default().clone()
    };

    let outcome = {
        let _guard = lock.lock().await;
        evict_corrupt_entity_file_locked(storage, entity_id).await
    };

    {
        let mut map = eviction_locks().lock();
        if let Some(existing) = map.get(entity_id) {
            if Arc::ptr_eq(existing, &lock) && Arc::strong_count(existing) == 2 {
                map.remove(entity_id);
            }
        }
    }

    outcome
}

async fn evict_corrupt_entity_file_locked(
    storage: &catalyrst_storage::ContentStorage,
    entity_id: &str,
) -> &'static str {
    let current = match storage.retrieve(entity_id).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return "the corrupt local copy was already removed; \
                    a later retry will re-download it";
        }
        Err(_) => {
            return "the local copy could not be re-verified; \
                    it was kept and a later retry will re-check it";
        }
    };

    if verify_cached_entity(&current, entity_id) != CachedEntityVerification::Mismatch {
        return "the local copy has since been replaced by a hash-valid one, which was kept";
    }

    match storage.delete_strict(entity_id).await {
        Ok(()) => {
            metrics::counter!("catalyrst_corrupt_entity_evictions_total").increment(1);
            "removed the corrupt local copy so it can be re-downloaded"
        }
        Err(_) => "could not remove the corrupt local copy; a later retry will attempt it again",
    }
}

pub async fn deploy_entity_streaming(
    client: &Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
    deployer: &LiveSyncDeployer,
    entity_id: &str,
    auth_chain: &AuthChain,
    servers: &[String],
    context: DeploymentContext,
    content_semaphore: Arc<Semaphore>,
    report: Option<&std::sync::Arc<super::batch_deployer::DeploymentReport>>,
) -> Result<(), SyncError> {
    download_file_with_retries(client, storage.as_ref(), entity_id, servers).await?;

    let entity_data =
        storage
            .retrieve(entity_id)
            .await?
            .ok_or_else(|| SyncError::EntityNotFound {
                entity_id: entity_id.to_string(),
            })?;

    let verification = verify_cached_entity(&entity_data, entity_id);
    if verification == CachedEntityVerification::Mismatch {
        warn!(
            entity_id,
            "Cached entity file failed content-hash verification"
        );
        let outcome = evict_corrupt_entity_file(storage.as_ref(), entity_id).await;
        return Err(SyncError::Other(format!(
            "The stored entity file for {entity_id} failed content-hash verification; {outcome}."
        )));
    }

    if entity_data.is_empty() {
        return Err(entity_parse_error(
            entity_id,
            verification,
            "the stored entity file was empty",
        ));
    }
    let hashes = extract_content_hashes(&entity_data)
        .map_err(|e| entity_parse_error(entity_id, verification, e))?;

    let mut tasks: tokio::task::JoinSet<Result<(), SyncError>> = tokio::task::JoinSet::new();

    for hash in hashes {
        if storage.exist(&hash).await? {
            continue;
        }

        let client = client.clone();
        let storage = storage.clone();
        let servers = servers.to_vec();
        let sem = content_semaphore.clone();

        tasks.spawn(async move {
            let _permit = sem.acquire().await.map_err(|_| SyncError::Stopped)?;
            download_file_with_retries(&client, storage.as_ref(), &hash, &servers).await
        });
    }

    let mut first_error: Option<SyncError> = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(SyncError::Other(format!("task join error: {}", e)));
                }
            }
        }
    }

    if let Some(e) = first_error {
        return Err(e);
    }

    deployer
        .deploy_entity(&entity_data, entity_id, auth_chain, context, report)
        .await?;

    Ok(())
}

fn entity_parse_error(
    entity_id: &str,
    verification: CachedEntityVerification,
    cause: impl std::fmt::Display,
) -> SyncError {
    let kept = match verification {
        CachedEntityVerification::Match => {
            "the stored bytes match the content hash, so the local copy was kept"
        }
        _ => {
            "the stored bytes could not be proven corrupt (unverifiable hash scheme), \
             so the local copy was kept"
        }
    };
    SyncError::Other(format!(
        "Failed to parse the downloaded entity file for {entity_id}; {kept}. Cause: {cause}"
    ))
}

fn extract_content_hashes(entity_data: &[u8]) -> Result<Vec<String>, serde_json::Error> {
    let entity: serde_json::Value = serde_json::from_slice(entity_data)?;
    let mut seen = HashSet::new();
    let mut hashes = Vec::new();

    if let Some(content) = entity.get("content").and_then(|c| c.as_array()) {
        for entry in content {
            if let Some(hash) = entry.get("hash").and_then(|h| h.as_str()) {
                if seen.insert(hash.to_string()) {
                    hashes.push(hash.to_string());
                }
            }
        }
    }

    let avatars = entity
        .pointer("/metadata/avatars")
        .or_else(|| entity.get("avatars"))
        .and_then(|a| a.as_array());

    if let Some(avatar_list) = avatars {
        for avatar_entry in avatar_list {
            let snapshots = avatar_entry.pointer("/avatar/snapshots");
            if let Some(obj) = snapshots.and_then(|s| s.as_object()) {
                for (_key, val) in obj {
                    if let Some(raw) = val.as_str() {
                        let hash = extract_hash_from_snapshot_value(raw);
                        if !hash.is_empty() && seen.insert(hash.to_string()) {
                            hashes.push(hash.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(hashes)
}

fn extract_hash_from_snapshot_value(s: &str) -> &str {
    if let Some(idx) = s.rfind("/contents/") {
        &s[idx + "/contents/".len()..]
    } else {
        s
    }
}

async fn download_file_with_retries(
    client: &Client,
    storage: &catalyrst_storage::ContentStorage,
    hash: &str,
    servers: &[String],
) -> Result<(), SyncError> {
    if storage.exist(hash).await? {
        return Ok(());
    }

    if servers.is_empty() {
        return Err(SyncError::Other("No servers available".into()));
    }

    let mut last_error = None;
    let start = rand::random_range(0..servers.len());
    for attempt in 0..MAX_DOWNLOAD_RETRIES {
        let server = &servers[(start + attempt as usize) % servers.len()];
        let url = format!("{}/contents/{}", server, hash);

        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let expected_len = resp.content_length();
                    let content_encoding = response_content_encoding(&resp);
                    if let Some(len) = expected_len {
                        if len as usize > MAX_BODY_BYTES {
                            warn!(
                                hash,
                                %server,
                                attempt,
                                content_length = len,
                                "Content body advertises size over cap, trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "content {} from {} exceeds {} byte cap (content-length {})",
                                hash, server, MAX_BODY_BYTES, len
                            )));
                            if attempt + 1 < MAX_DOWNLOAD_RETRIES {
                                tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS))
                                    .await;
                            }
                            continue;
                        }
                    }

                    let mut buf: Vec<u8> = Vec::new();
                    let mut stream = resp.bytes_stream();
                    let mut oversize = false;
                    let mut stream_err: Option<reqwest::Error> = None;
                    while let Some(chunk_res) = stream.next().await {
                        match chunk_res {
                            Ok(chunk) => {
                                if buf.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                                    oversize = true;
                                    break;
                                }
                                buf.extend_from_slice(&chunk);
                            }
                            Err(e) => {
                                stream_err = Some(e);
                                break;
                            }
                        }
                    }

                    if oversize {
                        warn!(
                            hash,
                            %server,
                            attempt,
                            bytes_so_far = buf.len(),
                            cap = MAX_BODY_BYTES,
                            "Content body exceeded cap mid-stream, trying next server"
                        );
                        last_error = Some(SyncError::Other(format!(
                            "content {} from {} exceeds {} byte cap",
                            hash, server, MAX_BODY_BYTES
                        )));
                        if attempt + 1 < MAX_DOWNLOAD_RETRIES {
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS))
                                .await;
                        }
                        continue;
                    }

                    if let Some(e) = stream_err {
                        warn!(hash, %server, attempt, error = %e, "Content stream error");
                        last_error = Some(SyncError::Http(e));
                        if attempt + 1 < MAX_DOWNLOAD_RETRIES {
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS))
                                .await;
                        }
                        continue;
                    }

                    if let Some(len) = expected_len {
                        if (buf.len() as u64) < len {
                            warn!(
                                hash,
                                %server,
                                attempt,
                                got = buf.len(),
                                expected = len,
                                "Content download TRUNCATED (short read), trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "content {} from {} truncated: got {} of {} bytes",
                                hash,
                                server,
                                buf.len(),
                                len
                            )));
                            if attempt + 1 < MAX_DOWNLOAD_RETRIES {
                                tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS))
                                    .await;
                            }
                            continue;
                        }
                    }

                    let decoded = match decode_content_encoding(
                        content_encoding.as_deref(),
                        buf,
                        MAX_BODY_BYTES,
                    ) {
                        Ok(decoded) => decoded,
                        Err(reason) => {
                            warn!(
                                hash,
                                %server,
                                attempt,
                                %reason,
                                "Content body transfer decoding failed, trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "content {} from {}: {}",
                                hash, server, reason
                            )));
                            if attempt + 1 < MAX_DOWNLOAD_RETRIES {
                                tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS))
                                    .await;
                            }
                            continue;
                        }
                    };
                    let bytes: bytes::Bytes = decoded.into();
                    if !catalyrst_hashing::verify_hash(&bytes, hash) {
                        warn!(hash, %server, attempt, "Downloaded content failed hash verification");
                        metrics::counter!("catalyrst_content_hash_mismatch_total").increment(1);
                        last_error = Some(SyncError::Other(format!(
                            "content hash mismatch for {} from {}",
                            hash, server
                        )));
                    } else {
                        let n = bytes.len() as u64;
                        storage.store(hash, bytes).await?;
                        debug!(hash, "Downloaded content file");
                        metrics::counter!("catalyrst_content_downloads_total", "result" => "ok")
                            .increment(1);
                        metrics::counter!("catalyrst_content_bytes_total").increment(n);
                        return Ok(());
                    }
                } else if resp.status().as_u16() == 404 {
                    last_error = Some(SyncError::Other(format!(
                        "404 fetching {} from {}",
                        hash, server
                    )));
                } else {
                    let status = resp.status();
                    warn!(hash, %server, %status, attempt, "Download failed");
                    last_error = Some(SyncError::Other(format!(
                        "HTTP {} fetching {}",
                        status, url
                    )));
                }
            }
            Err(e) => {
                warn!(hash, %server, error = %e, attempt, "Download request failed");
                last_error = Some(SyncError::Http(e));
            }
        }

        if attempt + 1 < MAX_DOWNLOAD_RETRIES {
            tokio::time::sleep(std::time::Duration::from_millis(RETRY_WAIT_MS)).await;
        }
    }

    Err(last_error
        .unwrap_or_else(|| SyncError::Other(format!("Failed to download {} after retries", hash))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_content_hashes() {
        let entity = serde_json::json!({
            "content": [
                {"file": "scene.json", "hash": "bafabc"},
                {"file": "model.glb", "hash": "bafdef"},
            ]
        });
        let data = serde_json::to_vec(&entity).unwrap();
        let hashes = extract_content_hashes(&data).unwrap();
        assert_eq!(hashes, vec!["bafabc", "bafdef"]);
    }

    #[test]
    fn test_extract_content_hashes_deduplicates() {
        let entity = serde_json::json!({
            "content": [
                {"file": "body.png", "hash": "bafbody"},
            ],
            "metadata": {
                "avatars": [{
                    "avatar": {
                        "snapshots": {
                            "face256": "bafface",
                            "body": "bafbody",
                        }
                    }
                }]
            }
        });
        let data = serde_json::to_vec(&entity).unwrap();
        let hashes = extract_content_hashes(&data).unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&"bafbody".to_string()));
        assert!(hashes.contains(&"bafface".to_string()));
    }

    #[test]
    fn test_extract_content_hashes_multi_avatar() {
        let entity = serde_json::json!({
            "content": [],
            "metadata": {
                "avatars": [
                    {"avatar": {"snapshots": {"face": "baf1"}}},
                    {"avatar": {"snapshots": {"face": "baf2"}}},
                ]
            }
        });
        let data = serde_json::to_vec(&entity).unwrap();
        let hashes = extract_content_hashes(&data).unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&"baf1".to_string()));
        assert!(hashes.contains(&"baf2".to_string()));
    }

    const ENTITY_JSON: &[u8] = br#"{"type":"scene","content":[]}"#;
    const CORRUPT_BYTES: &[u8] = br#"{"type":"scene","#;

    async fn temp_storage(tag: &str) -> (catalyrst_storage::ContentStorage, std::path::PathBuf) {
        let tmp = std::env::temp_dir().join(format!(
            "catalyrst-evict-{tag}-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        let storage = catalyrst_storage::ContentStorage::new(&tmp).await.unwrap();
        (storage, tmp)
    }

    #[test]
    fn test_verify_cached_entity_tri_state() {
        let v1 = catalyrst_hashing::hash_bytes_v1(ENTITY_JSON);
        let v0 = catalyrst_hashing::hash_bytes(ENTITY_JSON);

        assert_eq!(
            verify_cached_entity(ENTITY_JSON, &v1),
            CachedEntityVerification::Match
        );
        assert_eq!(
            verify_cached_entity(ENTITY_JSON, &v0),
            CachedEntityVerification::Match
        );
        assert_eq!(
            verify_cached_entity(CORRUPT_BYTES, &v1),
            CachedEntityVerification::Mismatch
        );
        assert_eq!(
            verify_cached_entity(CORRUPT_BYTES, &v0),
            CachedEntityVerification::Mismatch
        );

        assert_eq!(
            verify_cached_entity(CORRUPT_BYTES, "Qmnotavalidcid"),
            CachedEntityVerification::Unverifiable
        );
        let over_long_v1 = format!("ba{}", "a".repeat(70));
        assert_eq!(
            verify_cached_entity(CORRUPT_BYTES, &over_long_v1),
            CachedEntityVerification::Unverifiable
        );
        assert_eq!(
            verify_cached_entity(CORRUPT_BYTES, "zUnknownScheme"),
            CachedEntityVerification::Unverifiable
        );
    }

    #[tokio::test]
    async fn evict_removes_a_proven_corrupt_copy() {
        const MISMATCH_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"mismatch"}"#;

        let (storage, tmp) = temp_storage("mismatch").await;
        let entity_id = catalyrst_hashing::hash_bytes_v1(MISMATCH_JSON);
        storage
            .store(&entity_id, bytes::Bytes::from_static(CORRUPT_BYTES))
            .await
            .unwrap();

        let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
        assert_eq!(
            outcome,
            "removed the corrupt local copy so it can be re-downloaded"
        );
        assert!(!storage.exist(&entity_id).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn evict_keeps_a_healed_copy() {
        const HEALED_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"healed"}"#;

        let (storage, tmp) = temp_storage("healed").await;
        let entity_id = catalyrst_hashing::hash_bytes_v1(HEALED_JSON);
        storage
            .store(&entity_id, bytes::Bytes::from_static(HEALED_JSON))
            .await
            .unwrap();

        let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
        assert_eq!(
            outcome,
            "the local copy has since been replaced by a hash-valid one, which was kept"
        );
        assert!(storage.exist(&entity_id).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn evict_reports_an_already_removed_copy() {
        const GONE_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"gone"}"#;

        let (storage, tmp) = temp_storage("gone").await;
        let entity_id = catalyrst_hashing::hash_bytes_v1(GONE_JSON);

        let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
        assert_eq!(
            outcome,
            "the corrupt local copy was already removed; \
             a later retry will re-download it"
        );

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn concurrent_evictions_of_the_same_entity_serialize() {
        const RACE_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"race"}"#;

        let (storage, tmp) = temp_storage("race").await;
        let storage = Arc::new(storage);
        let entity_id = catalyrst_hashing::hash_bytes_v1(RACE_JSON);
        storage
            .store(&entity_id, bytes::Bytes::from_static(CORRUPT_BYTES))
            .await
            .unwrap();

        let mut join = tokio::task::JoinSet::new();
        for _ in 0..2 {
            let storage = storage.clone();
            let entity_id = entity_id.clone();
            join.spawn(async move { evict_corrupt_entity_file(&storage, &entity_id).await });
        }
        let mut outcomes: Vec<&'static str> = Vec::new();
        while let Some(res) = join.join_next().await {
            outcomes.push(res.unwrap());
        }
        outcomes.sort();

        assert!(outcomes.contains(&"removed the corrupt local copy so it can be re-downloaded"));
        assert!(!storage.exist(&entity_id).await.unwrap());
        assert!(!eviction_locks().lock().contains_key(&entity_id));

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[cfg(unix)]
    fn running_as_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    #[cfg(unix)]
    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[derive(Default)]
    struct CounterRecorder {
        counters: parking_lot::Mutex<HashMap<String, Arc<metrics::atomics::AtomicU64>>>,
    }

    impl CounterRecorder {
        fn value(&self, name: &str) -> u64 {
            self.counters
                .lock()
                .get(name)
                .map(|c| c.load(std::sync::atomic::Ordering::Acquire))
                .unwrap_or(0)
        }
    }

    impl metrics::Recorder for CounterRecorder {
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }
        fn register_counter(
            &self,
            key: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Counter {
            let handle = self
                .counters
                .lock()
                .entry(key.name().to_string())
                .or_default()
                .clone();
            metrics::Counter::from_arc(handle)
        }
        fn register_gauge(&self, _: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
            metrics::Gauge::noop()
        }
        fn register_histogram(
            &self,
            _: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Histogram {
            metrics::Histogram::noop()
        }
    }

    #[cfg(unix)]
    #[test]
    fn evict_reports_failure_and_skips_metric_when_delete_fails() {
        if running_as_root() {
            eprintln!("skipping: permissions do not bind when running as root");
            return;
        }

        let recorder = CounterRecorder::default();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        const DELFAIL_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"delfail"}"#;

        metrics::with_local_recorder(&recorder, || {
            rt.block_on(async {
                let (storage, tmp) = temp_storage("delfail").await;
                let entity_id = catalyrst_hashing::hash_bytes_v1(DELFAIL_JSON);
                storage
                    .store(&entity_id, bytes::Bytes::from_static(CORRUPT_BYTES))
                    .await
                    .unwrap();

                let shard_dir = storage
                    .root()
                    .join(catalyrst_storage::hex_prefix(&entity_id));
                set_mode(&shard_dir, 0o555);

                let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
                assert_eq!(
                    outcome,
                    "could not remove the corrupt local copy; \
                     a later retry will attempt it again"
                );
                set_mode(&shard_dir, 0o755);
                assert!(
                    storage.exist(&entity_id).await.unwrap(),
                    "the copy must still be on disk"
                );
                assert_eq!(
                    recorder.value("catalyrst_corrupt_entity_evictions_total"),
                    0,
                    "a failed delete must not count as an eviction"
                );

                let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
                assert_eq!(
                    outcome,
                    "removed the corrupt local copy so it can be re-downloaded"
                );
                assert_eq!(
                    recorder.value("catalyrst_corrupt_entity_evictions_total"),
                    1
                );

                let _ = tokio::fs::remove_dir_all(&tmp).await;
            })
        });
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn evict_reports_fault_not_removal_when_reverify_cannot_read() {
        if running_as_root() {
            eprintln!("skipping: permissions do not bind when running as root");
            return;
        }

        const REVERIFY_FAULT_JSON: &[u8] = br#"{"type":"scene","content":[],"t":"reverify"}"#;

        let (storage, tmp) = temp_storage("reverify-fault").await;
        let entity_id = catalyrst_hashing::hash_bytes_v1(REVERIFY_FAULT_JSON);
        storage
            .store(&entity_id, bytes::Bytes::from_static(CORRUPT_BYTES))
            .await
            .unwrap();

        let shard_dir = storage
            .root()
            .join(catalyrst_storage::hex_prefix(&entity_id));
        set_mode(&shard_dir, 0o000);

        let outcome = evict_corrupt_entity_file(&storage, &entity_id).await;
        assert_eq!(
            outcome,
            "the local copy could not be re-verified; \
             it was kept and a later retry will re-check it"
        );

        set_mode(&shard_dir, 0o755);
        assert!(storage.exist(&entity_id).await.unwrap());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[test]
    fn entity_parse_error_states_why_the_copy_was_kept() {
        let err = entity_parse_error(
            "bafentity",
            CachedEntityVerification::Match,
            "the stored entity file was empty",
        );
        assert_eq!(
            err.to_string(),
            "Failed to parse the downloaded entity file for bafentity; the stored bytes \
             match the content hash, so the local copy was kept. Cause: the stored \
             entity file was empty"
        );

        let err = entity_parse_error(
            "bafentity",
            CachedEntityVerification::Unverifiable,
            "expected value at line 1 column 1",
        );
        assert_eq!(
            err.to_string(),
            "Failed to parse the downloaded entity file for bafentity; the stored bytes \
             could not be proven corrupt (unverifiable hash scheme), so the local copy \
             was kept. Cause: expected value at line 1 column 1"
        );
    }

    // The live peer.dclnodes.io failure mode: content-addressed blobs served with
    // `Content-Encoding: gzip` regardless of Accept-Encoding, CID computed over the DECODED
    // bytes. The download path must undo the transfer coding before hashing and must store the
    // decoded representation.
    #[tokio::test]
    async fn gzip_labeled_content_decodes_then_hashes_and_stores_decoded() {
        const PAYLOAD: &[u8] = br#"{"type":"scene","content":[],"t":"gzip-wire"}"#;
        let cid = catalyrst_hashing::hash_bytes_v1(PAYLOAD);
        let wire = crate::sync::content_encoding::gzip(PAYLOAD);
        let (base, server) =
            crate::sync::test_support::spawn_content_server(wire, Some("gzip")).await;
        let (storage, tmp) = temp_storage("gzip-wire").await;

        download_file_with_retries(&reqwest::Client::new(), &storage, &cid, &[base])
            .await
            .expect("a gzip-labeled body whose decoded bytes match the CID must be accepted");

        let stored = storage.retrieve(&cid).await.unwrap().unwrap();
        assert_eq!(
            &stored[..],
            PAYLOAD,
            "the stored bytes must be the decoded representation"
        );
        assert!(catalyrst_hashing::verify_hash(&stored, &cid));

        server.abort();
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn gzip_labeled_body_whose_raw_bytes_match_the_cid_is_rejected() {
        const PAYLOAD: &[u8] = br#"{"type":"scene","content":[],"t":"raw-labeled"}"#;
        let cid = catalyrst_hashing::hash_bytes_v1(PAYLOAD);
        let (base, server) =
            crate::sync::test_support::spawn_content_server(PAYLOAD.to_vec(), Some("gzip")).await;
        let (storage, tmp) = temp_storage("raw-labeled").await;

        let err = download_file_with_retries(&reqwest::Client::new(), &storage, &cid, &[base])
            .await
            .expect_err("a non-gzip body labeled gzip must fail transfer decoding");
        assert!(
            err.to_string().contains("gzip transfer decoding failed"),
            "{err}"
        );
        assert!(!storage.exist(&cid).await.unwrap());

        server.abort();
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn corrupt_gzip_content_still_fails_hash_verification() {
        const PAYLOAD: &[u8] = br#"{"type":"scene","content":[],"t":"expected"}"#;
        const OTHER: &[u8] = br#"{"type":"scene","content":[],"t":"corrupted"}"#;
        let cid = catalyrst_hashing::hash_bytes_v1(PAYLOAD);
        let wire = crate::sync::content_encoding::gzip(OTHER);
        let (base, server) =
            crate::sync::test_support::spawn_content_server(wire, Some("gzip")).await;
        let (storage, tmp) = temp_storage("gzip-corrupt").await;

        let err = download_file_with_retries(&reqwest::Client::new(), &storage, &cid, &[base])
            .await
            .expect_err("decoded bytes that mismatch the CID must still be rejected");
        assert!(err.to_string().contains("content hash mismatch"), "{err}");
        assert!(!storage.exist(&cid).await.unwrap());

        server.abort();
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[test]
    fn test_extract_content_hashes_url_snapshots() {
        let entity = serde_json::json!({
            "content": [],
            "metadata": {
                "avatars": [{
                    "avatar": {
                        "snapshots": {
                            "face": "https://peer.decentraland.org/content/contents/bafurlhash"
                        }
                    }
                }]
            }
        });
        let data = serde_json::to_vec(&entity).unwrap();
        let hashes = extract_content_hashes(&data).unwrap();
        assert_eq!(hashes, vec!["bafurlhash"]);
    }
}
