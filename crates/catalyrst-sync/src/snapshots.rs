use std::collections::HashSet;
use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use tracing::{debug, info, warn};

use crate::{
    ContentStorage, ProcessedSnapshotStore, SnapshotMetadata, SnapshotStorageCheck, SyncDeployment,
    SyncError, Timestamp,
};

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024 * 1024;

pub async fn fetch_snapshots(
    client: &Client,
    server: &str,
    max_retries: u32,
) -> Result<Vec<SnapshotMetadata>, SyncError> {
    let url = format!("{}/snapshots", server);

    let mut last_error = None;
    for attempt in 0..max_retries {
        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let mut snapshots: Vec<SnapshotMetadata> = resp.json().await?;
                    snapshots.sort_by(|a, b| {
                        b.time_range.end_timestamp.cmp(&a.time_range.end_timestamp)
                    });
                    return Ok(snapshots);
                } else {
                    let status = resp.status();
                    warn!(url = %url, %status, attempt, "Snapshot fetch failed");
                    last_error = Some(SyncError::Other(format!(
                        "HTTP {} fetching snapshots from {}",
                        status, server
                    )));
                }
            }
            Err(e) => {
                warn!(url = %url, error = %e, attempt, "Snapshot fetch request failed");
                last_error = Some(SyncError::Http(e));
            }
        }

        if attempt + 1 < max_retries {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| {
        SyncError::Other(format!(
            "Failed to fetch snapshots from {} after {} retries",
            server, max_retries
        ))
    }))
}

const MAX_DECOMPRESSED_SNAPSHOT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn decompress_snapshot(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        use std::io::Read;

        let mut decoder =
            flate2::read::GzDecoder::new(data).take(MAX_DECOMPRESSED_SNAPSHOT_BYTES + 1);
        let mut buf = Vec::new();
        match decoder.read_to_end(&mut buf) {
            Ok(_) => {
                if buf.len() as u64 > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
                    warn!(
                        bytes = buf.len(),
                        cap = MAX_DECOMPRESSED_SNAPSHOT_BYTES,
                        "Snapshot decompressed to > cap, refusing"
                    );
                    return std::borrow::Cow::Owned(Vec::new());
                }
                return std::borrow::Cow::Owned(buf);
            }
            Err(e) => {
                warn!(error = %e, "Failed to decompress snapshot");
            }
        }
    }
    std::borrow::Cow::Borrowed(data)
}

pub fn parse_snapshot_entities(data: &[u8]) -> Vec<SyncDeployment> {
    let text_bytes = decompress_snapshot(data);
    let text = String::from_utf8_lossy(&text_bytes);
    let mut deployments = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            match serde_json::from_str::<SyncDeployment>(trimmed) {
                Ok(deployment) => deployments.push(deployment),
                Err(e) => {
                    warn!(
                        error = %e,
                        line_preview = &trimmed[..trimmed.len().min(100)],
                        "Invalid deployment in snapshot file, skipping"
                    );
                }
            }
        }
    }

    deployments
}

pub async fn download_snapshot_files(
    client: &Client,
    storage: Arc<dyn ContentStorage>,
    snapshots: &[(String, HashSet<String>)],
    max_retries: u32,
    retry_wait_ms: u64,
) {
    let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    for (hash, servers) in snapshots {
        let client = client.clone();
        let storage = storage.clone();
        let hash = hash.clone();
        let servers = servers.clone();

        tasks.spawn(async move {
            if let Err(e) = download_snapshot_file(
                &client,
                storage.as_ref(),
                &hash,
                &servers,
                max_retries,
                retry_wait_ms,
            )
            .await
            {
                warn!(snapshot_hash = %hash, error = %e, "Failed to pre-download snapshot");
            } else {
                info!(snapshot_hash = %hash, "Snapshot pre-downloaded");
            }
        });
    }

    while let Some(result) = tasks.join_next().await {
        if let Err(e) = result {
            warn!(error = %e, "Snapshot download task panicked");
        }
    }
}

pub async fn should_deploy_snapshot(
    processed_store: &dyn ProcessedSnapshotStore,
    snapshot_store: &dyn SnapshotStorageCheck,
    genesis_timestamp: Timestamp,
    snapshot_hash: &str,
    greatest_end_timestamp: Timestamp,
    replaced_snapshot_hash_groups: &[Vec<String>],
) -> Result<bool, SyncError> {
    let mut all_hashes = vec![snapshot_hash.to_string()];
    for group in replaced_snapshot_hash_groups {
        all_hashes.extend(group.iter().cloned());
    }

    let processed = processed_store.filter_processed(&all_hashes).await?;

    let snapshot_was_processed = processed.contains(snapshot_hash);

    let a_replaced_group_was_processed = replaced_snapshot_hash_groups
        .iter()
        .any(|group| !group.is_empty() && group.iter().all(|h| processed.contains(h)));

    if !snapshot_was_processed {
        if !a_replaced_group_was_processed {
            if greatest_end_timestamp > genesis_timestamp {
                let already_downloaded = snapshot_store.has(snapshot_hash).await?;
                return Ok(!already_downloaded);
            }
            return Ok(false);
        } else {
            processed_store.mark_processed(snapshot_hash).await?;
        }
    }

    Ok(false)
}

pub async fn deploy_entities_from_snapshot<D>(
    client: &Client,
    storage: &dyn ContentStorage,
    deployer: &D,
    snapshot_hash: &str,
    servers: &HashSet<String>,
    genesis_timestamp: Timestamp,
    max_retries: u32,
    retry_wait_ms: u64,
    entity_type_filter: Option<&HashSet<String>>,
    should_stop: impl Fn() -> bool,
) -> Result<(), SyncError>
where
    D: crate::batch_deployer::DeploymentScheduler,
{
    let server_list: Vec<String> = servers.iter().cloned().collect();

    download_snapshot_file(
        client,
        storage,
        snapshot_hash,
        servers,
        max_retries,
        retry_wait_ms,
    )
    .await?;

    let data = storage
        .retrieve(snapshot_hash)
        .await?
        .ok_or_else(|| SyncError::EntityNotFound {
            entity_id: snapshot_hash.to_string(),
        })?;

    let text_bytes = decompress_snapshot(&data);
    let text = String::from_utf8_lossy(&text_bytes);

    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    // Per-type concurrency. Non-scene entities (profile / wearable / emote /
    // outfits / store) are single-pointer with disjoint keys → last-write-wins
    // with no cross-entity pointer collisions, safe to deploy with high
    // concurrency. Scenes are multi-pointer (LAND parcels) and carry heavy
    // content, so they stay ~serial to bound memory. Correctness is guaranteed
    // regardless by the deployer (sorted per-pointer advisory locks +
    // recency-conditional active_pointers upsert + in-batch pointer dedup), so
    // these knobs only trade throughput against memory, never correctness.
    let nonscene_concurrency: usize = std::env::var("SYNC_NONSCENE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(64);
    let scene_concurrency: usize = std::env::var("SYNC_SCENE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(1);

    let total = AtomicU64::new(0);
    let num_scheduled = AtomicU64::new(0);
    let num_skipped_by_filter = AtomicU64::new(0);
    let num_parse_errors = AtomicU64::new(0);
    let stopped = AtomicBool::new(false);
    let scenes: std::sync::Mutex<Vec<SyncDeployment>> = std::sync::Mutex::new(Vec::new());

    // Parse + pre-filter one snapshot line into a deployable entity, or None.
    let parse = |line: &str| -> Option<SyncDeployment> {
        let trimmed = line.trim();
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            return None;
        }
        total.fetch_add(1, Ordering::Relaxed);
        let deployment: SyncDeployment = match serde_json::from_str(trimmed) {
            Ok(d) => d,
            Err(e) => {
                if num_parse_errors.fetch_add(1, Ordering::Relaxed) < 5 {
                    warn!(snapshot_hash, error = %e, "Skipping unparseable snapshot entry");
                }
                return None;
            }
        };
        if deployment.entity_timestamp < genesis_timestamp {
            return None;
        }
        if let Some(filter) = entity_type_filter {
            if !filter.contains(&deployment.entity_type) {
                num_skipped_by_filter.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        }
        Some(deployment)
    };

    // Captured-by-copy references so the concurrent futures stay Send and don't
    // borrow `should_stop` (which need not be Sync — it's polled only on the
    // sequential producer side below).
    let server_list_ref: &[String] = &server_list;
    let scheduled = &num_scheduled;
    let stop_flag = &stopped;
    let scenes_ref = &scenes;

    // Lane 1: stream all entities, deploy non-scenes concurrently, defer scenes.
    futures::stream::iter(text.lines())
        .filter_map(|line| {
            if should_stop() {
                stop_flag.store(true, Ordering::Relaxed);
            }
            let parsed = if stop_flag.load(Ordering::Relaxed) {
                None
            } else {
                parse(line)
            };
            async move { parsed }
        })
        .for_each_concurrent(nonscene_concurrency, |deployment| async move {
            if stop_flag.load(Ordering::Relaxed) {
                return;
            }
            if deployment.entity_type == "scene" {
                scenes_ref.lock().unwrap().push(deployment);
                return;
            }
            match deployer
                .schedule_entity_deployment(deployment, server_list_ref)
                .await
            {
                Ok(()) => {
                    scheduled.fetch_add(1, Ordering::Relaxed);
                }
                Err(SyncError::Stopped) => stop_flag.store(true, Ordering::Relaxed),
                Err(e) => warn!(snapshot_hash, error = %e, "Failed to schedule entity deployment"),
            }
        })
        .await;

    // Lane 2: scenes — ~serial (bounded by scene_concurrency) for memory safety.
    let scene_batch = std::mem::take(&mut *scenes.lock().unwrap());
    futures::stream::iter(scene_batch)
        .for_each_concurrent(scene_concurrency, |deployment| async move {
            if stop_flag.load(Ordering::Relaxed) {
                return;
            }
            match deployer
                .schedule_entity_deployment(deployment, server_list_ref)
                .await
            {
                Ok(()) => {
                    scheduled.fetch_add(1, Ordering::Relaxed);
                }
                Err(SyncError::Stopped) => stop_flag.store(true, Ordering::Relaxed),
                Err(e) => warn!(snapshot_hash, error = %e, "Failed to schedule entity deployment"),
            }
        })
        .await;

    if stopped.load(Ordering::Relaxed) {
        return Err(SyncError::Stopped);
    }

    let total = total.load(Ordering::Relaxed);
    let num_scheduled = num_scheduled.load(Ordering::Relaxed);
    let num_skipped_by_filter = num_skipped_by_filter.load(Ordering::Relaxed);
    let num_parse_errors = num_parse_errors.load(Ordering::Relaxed);

    if num_parse_errors > 0 {
        warn!(
            snapshot_hash,
            total, num_parse_errors, "Snapshot had unparseable entries"
        );
    }

    info!(
        snapshot_hash,
        total,
        num_scheduled,
        num_skipped_by_filter,
        num_parse_errors,
        nonscene_concurrency,
        scene_concurrency,
        "Snapshot scheduled"
    );

    Ok(())
}

async fn download_snapshot_file(
    client: &Client,
    storage: &dyn ContentStorage,
    snapshot_hash: &str,
    servers: &HashSet<String>,
    max_retries: u32,
    retry_wait_ms: u64,
) -> Result<(), SyncError> {
    if storage.exists(snapshot_hash).await? {
        debug!(snapshot_hash, "Snapshot already in storage");
        return Ok(());
    }

    let server_list: Vec<&String> = servers.iter().collect();
    if server_list.is_empty() {
        return Err(SyncError::NoServers);
    }

    let mut last_error = None;
    for retry in 0..max_retries {
        let server = server_list[retry as usize % server_list.len()];
        let url = format!("{}/contents/{}", server, snapshot_hash);

        // No per-request total timeout: snapshot files are large (100s of MB) and
        // the shared client intentionally has no total cap — its read_timeout
        // catches stalled connections without truncating a slow large download.
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let expected_len = resp.content_length();
                    if let Some(len) = expected_len {
                        if len as usize > MAX_BODY_BYTES {
                            warn!(
                                snapshot_hash,
                                %server,
                                retry,
                                content_length = len,
                                "Snapshot body advertises size over cap, trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "snapshot {} from {} exceeds {} byte cap (content-length {})",
                                snapshot_hash, server, MAX_BODY_BYTES, len
                            )));
                            if retry + 1 < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
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
                            snapshot_hash,
                            %server,
                            retry,
                            bytes_so_far = buf.len(),
                            cap = MAX_BODY_BYTES,
                            "Snapshot body exceeded cap mid-stream, trying next server"
                        );
                        last_error = Some(SyncError::Other(format!(
                            "snapshot {} from {} exceeds {} byte cap",
                            snapshot_hash, server, MAX_BODY_BYTES
                        )));
                        if retry + 1 < max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
                                .await;
                        }
                        continue;
                    }

                    if let Some(e) = stream_err {
                        warn!(snapshot_hash, %server, error = %e, retry, "Snapshot stream error");
                        last_error = Some(SyncError::Http(e));
                        if retry + 1 < max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
                                .await;
                        }
                        continue;
                    }

                    // Explicit truncation signal: a short body (clean EOF before
                    // Content-Length) would otherwise fail the hash check with an
                    // opaque "hash verification failed". Report the real cause.
                    if let Some(len) = expected_len {
                        if (buf.len() as u64) < len {
                            warn!(
                                snapshot_hash,
                                %server,
                                retry,
                                got = buf.len(),
                                expected = len,
                                "Snapshot download TRUNCATED (short read), trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "snapshot {} from {} truncated: got {} of {} bytes",
                                snapshot_hash,
                                server,
                                buf.len(),
                                len
                            )));
                            if retry + 1 < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
                                    .await;
                            }
                            continue;
                        }
                    }

                    let bytes: bytes::Bytes = buf.into();

                    if !catalyrst_hashing::verify_hash(&bytes, snapshot_hash) {
                        warn!(
                            snapshot_hash,
                            %server,
                            retry,
                            bytes = bytes.len(),
                            "Snapshot content failed hash verification, trying next server"
                        );
                        last_error = Some(SyncError::Other(format!(
                            "snapshot hash mismatch for {} from {}",
                            snapshot_hash, server
                        )));
                    } else {
                        info!(snapshot_hash, bytes = bytes.len(), "Snapshot downloaded");
                        storage.store(snapshot_hash, bytes).await?;
                        return Ok(());
                    }
                } else {
                    let status = resp.status();
                    warn!(snapshot_hash, %server, %status, retry, "Snapshot download failed");
                    last_error = Some(SyncError::Other(format!(
                        "HTTP {} downloading snapshot {} from {}",
                        status, snapshot_hash, server
                    )));
                }
            }
            Err(e) => {
                warn!(snapshot_hash, %server, error = %e, retry, "Snapshot download request failed");
                last_error = Some(SyncError::Http(e));
            }
        }

        if retry + 1 < max_retries {
            tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| {
        SyncError::Other(format!(
            "Failed to download snapshot {} after {} retries",
            snapshot_hash, max_retries
        ))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_snapshot_entities_basic() {
        let data = b"### Decentraland json snapshot\n\
{\"entityId\":\"abc\",\"entityType\":\"scene\",\"pointers\":[\"0,0\"],\"authChain\":[{\"type\":\"SIGNER\",\"payload\":\"0xabc\"}],\"entityTimestamp\":1000}\n\
{\"entityId\":\"def\",\"entityType\":\"profile\",\"pointers\":[\"0xdef\"],\"authChain\":[{\"type\":\"SIGNER\",\"payload\":\"0xdef\"}],\"entityTimestamp\":2000}\n";

        let deployments = parse_snapshot_entities(data);
        assert_eq!(deployments.len(), 2);
        assert_eq!(deployments[0].entity_id, "abc");
        assert_eq!(deployments[1].entity_id, "def");
    }

    #[test]
    fn test_parse_snapshot_entities_skips_invalid() {
        let data = b"### header\n\
{\"entityId\":\"abc\",\"entityType\":\"scene\",\"pointers\":[],\"authChain\":[],\"entityTimestamp\":1000}\n\
not valid json\n\
{\"entityId\":\"def\",\"entityType\":\"profile\",\"pointers\":[],\"authChain\":[],\"entityTimestamp\":2000}\n";

        let deployments = parse_snapshot_entities(data);
        assert_eq!(deployments.len(), 2);
    }

    #[test]
    fn test_parse_snapshot_empty() {
        let deployments = parse_snapshot_entities(b"### Decentraland json snapshot\n");
        assert!(deployments.is_empty());
    }
}
