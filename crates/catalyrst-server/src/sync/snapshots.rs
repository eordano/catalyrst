use std::collections::HashSet;
use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use tracing::{debug, info, warn};

pub use catalyrst_types::snapshot::{decompress_snapshot, parse_snapshot_entities};

use super::backends::LiveProcessedSnapshotStore;
use super::batch_deployer::BatchDeployer;
use super::{SnapshotMetadata, SyncDeployment, SyncError, Timestamp};

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

pub async fn download_snapshot_files(
    client: &Client,
    storage: Arc<catalyrst_storage::ContentStorage>,
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
    processed_store: &LiveProcessedSnapshotStore,
    snapshot_store: &catalyrst_storage::SnapshotStorage,
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
                let already_downloaded = snapshot_store.exist(snapshot_hash).await?;
                return Ok(!already_downloaded);
            }
            return Ok(false);
        } else {
            processed_store.mark_processed(snapshot_hash).await?;
        }
    }

    Ok(false)
}

pub async fn deploy_entities_from_snapshot(
    client: &Client,
    storage: &catalyrst_storage::ContentStorage,
    deployer: &BatchDeployer,
    snapshot_hash: &str,
    servers: &HashSet<String>,
    genesis_timestamp: Timestamp,
    max_retries: u32,
    retry_wait_ms: u64,
    entity_type_filter: Option<&HashSet<String>>,
    should_stop: impl Fn() -> bool,
) -> Result<(), SyncError> {
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

    let server_list_ref: &[String] = &server_list;
    let scheduled = &num_scheduled;
    let stop_flag = &stopped;
    let scenes_ref = &scenes;

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
    storage: &catalyrst_storage::ContentStorage,
    snapshot_hash: &str,
    servers: &HashSet<String>,
    max_retries: u32,
    retry_wait_ms: u64,
) -> Result<(), SyncError> {
    if storage.exist(snapshot_hash).await? {
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
