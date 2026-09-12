use std::collections::HashSet;
use std::sync::Arc;

use catalyrst_commons::http::read_body_capped;
use futures::StreamExt;
use reqwest::Client;
use tracing::{debug, info, warn};

pub use catalyrst_types::snapshot::{decompress_snapshot, parse_snapshot_entities};

use super::batch_deployer::{BatchDeployer, DeploymentReport};
use super::content_encoding::{decode_content_encoding, response_content_encoding};
use super::pointer_changes::is_usable_timestamp;
use super::{SnapshotMetadata, SyncDeployment, SyncError, TimeRange, Timestamp};

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Upstream MAX_REPLACED_SNAPSHOT_HASHES. Every entry lands in the batched processed-snapshots
/// lookup, so a pathological list sizes that query; real snapshots replace tens.
const MAX_REPLACED_SNAPSHOT_HASHES: usize = 1000;

/// How many invalid /snapshots entries are logged in detail per response before the rest
/// collapse into one summary line -- the body is attacker-sized.
const MAX_INVALID_SNAPSHOT_LOGS: usize = 5;

/// Port of upstream 53e9c07's `SnapshotsFromServer`. Each snapshot stands for a whole time range, so
/// a discarded entry is a range nothing else covers, and treating the surviving subset as the
/// server's complete history would advance the frontier past those entities forever: callers that
/// record sync progress must check `discarded`, not just read `snapshots`.
#[derive(Debug, Clone)]
pub struct SnapshotsFromServer {
    pub snapshots: Vec<SnapshotMetadata>,
    pub discarded: usize,
}

/// Upstream `isValidSnapshotMetadata`. Snapshot metadata comes from untrusted servers, and
/// `time_range.end_timestamp` is what bootstrap installs as the server's last-snapshot timestamp and,
/// one poll boundary later, the durable GREATEST-monotonic frontier -- the same hazard the
/// pointer-changes timestamps are checked for.
///
/// `number_of_entities` and `generation_timestamp` are deliberately NOT validated (they default to 0
/// when missing or wrong-typed): nothing downstream reads them, and rejecting an entry over an unread
/// field would silently stop syncing from a server that reports, say, `numberOfEntities: "5"`.
pub(crate) fn parse_snapshot_metadata(
    value: &serde_json::Value,
    now_ms: Timestamp,
) -> Option<SnapshotMetadata> {
    let hash = value.get("hash")?.as_str()?;
    if !catalyrst_storage::is_canonical_content_id(hash) {
        return None;
    }

    let time_range = value.get("timeRange")?;
    let init_timestamp = time_range.get("initTimestamp")?.as_i64()?;
    let end_timestamp = time_range.get("endTimestamp")?.as_i64()?;
    if !is_usable_timestamp(init_timestamp, now_ms)
        || !is_usable_timestamp(end_timestamp, now_ms)
        || init_timestamp > end_timestamp
    {
        return None;
    }

    let replaced_snapshot_hashes = match value.get("replacedSnapshotHashes") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(hashes)) => {
            if hashes.len() > MAX_REPLACED_SNAPSHOT_HASHES {
                return None;
            }
            let mut replaced = Vec::with_capacity(hashes.len());
            for h in hashes {
                let h = h.as_str()?;
                if !catalyrst_storage::is_canonical_content_id(h) {
                    return None;
                }
                replaced.push(h.to_string());
            }
            Some(replaced)
        }
        Some(_) => return None,
    };

    Some(SnapshotMetadata {
        hash: hash.to_string(),
        time_range: TimeRange {
            init_timestamp,
            end_timestamp,
        },
        number_of_entities: value
            .get("numberOfEntities")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        replaced_snapshot_hashes,
        generation_timestamp: value
            .get("generationTimestamp")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    })
}

pub(crate) fn parse_snapshots_response(
    items: &[serde_json::Value],
    now_ms: Timestamp,
    server: &str,
) -> SnapshotsFromServer {
    let mut snapshots = Vec::with_capacity(items.len());
    let mut discarded = 0usize;
    for item in items {
        match parse_snapshot_metadata(item, now_ms) {
            Some(snapshot) => snapshots.push(snapshot),
            None => {
                discarded += 1;
                if discarded <= MAX_INVALID_SNAPSHOT_LOGS {
                    let mut preview = item.to_string();
                    preview.truncate(512);
                    warn!(server, entry = %preview, "Ignoring invalid snapshot metadata entry");
                }
            }
        }
    }
    if discarded > MAX_INVALID_SNAPSHOT_LOGS {
        warn!(
            server,
            total = discarded,
            "Ignored additional invalid snapshot metadata entries"
        );
    }
    snapshots.sort_by_key(|s| std::cmp::Reverse(s.time_range.end_timestamp));
    SnapshotsFromServer {
        snapshots,
        discarded,
    }
}

pub async fn fetch_snapshots(
    client: &Client,
    server: &str,
    max_retries: u32,
) -> Result<SnapshotsFromServer, SyncError> {
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
                    let items: Vec<serde_json::Value> = resp.json().await?;
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    return Ok(parse_snapshots_response(&items, now_ms, server));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotDecision {
    /// Unless the node's own snapshot store already has it -- the one async check the caller still
    /// owns; its result never changes within a pass.
    Deploy,
    /// Some advertised replacement group is fully processed, so this snapshot's content is
    /// already covered: persist a processed mark for it and skip.
    MarkProcessed,
    /// Already processed, or entirely older than the genesis timestamp.
    Skip,
}

/// Pure core of upstream's `decideSnapshotDeploymentFromProcessedSet`, taking an already-fetched set
/// of processed hashes so a pass costs as few storage round trips as `filter_processed_in_chunks`
/// needs rather than one per snapshot.
///
/// `replaced_groups` carries one group per advertising server, and `.any()` over them matches
/// upstream's `.some()`: two peers advertising the same content hash with different replacement
/// histories must reach the same verdict regardless of which was fetched first.
///
/// On MarkProcessed the hash is added to `processed` -- mutating the caller's set is what makes the
/// snapshot that replaces THIS one skippable later in the same pass; see [`decide_snapshot_pass`].
pub(crate) fn decide_snapshot_deployment_from_processed_set(
    processed: &mut HashSet<String>,
    genesis_timestamp: Timestamp,
    snapshot_hash: &str,
    greatest_end_timestamp: Timestamp,
    replaced_groups: &[Vec<String>],
) -> SnapshotDecision {
    let snapshot_was_processed = processed.contains(snapshot_hash);
    let a_replaced_group_was_processed = replaced_groups
        .iter()
        .any(|group| !group.is_empty() && group.iter().all(|h| processed.contains(h)));

    if !snapshot_was_processed {
        if !a_replaced_group_was_processed {
            if greatest_end_timestamp > genesis_timestamp {
                return SnapshotDecision::Deploy;
            }
            return SnapshotDecision::Skip;
        }
        processed.insert(snapshot_hash.to_string());
        return SnapshotDecision::MarkProcessed;
    }
    SnapshotDecision::Skip
}

/// Fixed point of the deployment decision over one pass's candidates (upstream 53e9c07's pass loop in
/// `syncFromSnapshotsExclusively`). A mark is exactly what makes the snapshot replacing IT skippable
/// in turn, so a single pass over a replacement chain (h2 replaces h1 replaces a processed h0)
/// collapses only one link and deploys the tail for nothing. The loop terminates because "already
/// processed" and "older than genesis" drop out for good the first time, and every other pass either
/// marks at least one snapshot or ends it.
///
/// `candidates` is `(hash, greatest_end_timestamp_across_advertisers, replaced-group-per-advertiser)`.
/// The caller persists `newly_marked` and applies its own snapshot-store check to `to_deploy`.
/// Iteration order follows `candidates`, so callers pass a deterministic order.
pub(crate) fn decide_snapshot_pass(
    candidates: &[(String, Timestamp, Vec<Vec<String>>)],
    processed: &mut HashSet<String>,
    genesis_timestamp: Timestamp,
) -> (Vec<String>, Vec<String>) {
    let mut newly_marked: Vec<String> = Vec::new();
    let mut still_deployable: Vec<&(String, Timestamp, Vec<Vec<String>>)> =
        candidates.iter().collect();
    loop {
        let marked_before = processed.len();
        let mut next_round = Vec::with_capacity(still_deployable.len());
        for candidate in still_deployable {
            let (hash, greatest_end, groups) = candidate;
            match decide_snapshot_deployment_from_processed_set(
                processed,
                genesis_timestamp,
                hash,
                *greatest_end,
                groups,
            ) {
                SnapshotDecision::Deploy => next_round.push(candidate),
                SnapshotDecision::MarkProcessed => newly_marked.push(hash.clone()),
                SnapshotDecision::Skip => {}
            }
        }
        if processed.len() == marked_before {
            return (
                next_round.into_iter().map(|(h, _, _)| h.clone()).collect(),
                newly_marked,
            );
        }
        still_deployable = next_round;
    }
}

/// The same timestamp-plausibility gate /pointer-changes deltas get (upstream applies
/// `isUsableTimestamp` at both ingestion points plus /snapshots metadata).
fn usable_snapshot_line(deployment: &SyncDeployment, now_ms: Timestamp) -> bool {
    is_usable_timestamp(deployment.entity_timestamp, now_ms)
        && deployment
            .local_timestamp
            .is_none_or(|ts| is_usable_timestamp(ts, now_ms))
}

/// Err when the snapshot could not be fully handed off -- download failure, unreadable lines, or
/// scheduling errors -- so the caller keeps the advertising servers in snapshot bootstrap with their
/// timestamps held back instead of advancing past entities that were never deployed. Readable
/// entities are still scheduled first.
///
/// `report` accumulates scheduled-vs-acknowledged counts; the caller re-checks it after the deployer
/// drains, since an Ok return only proves everything was SCHEDULED.
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
    report: &std::sync::Arc<DeploymentReport>,
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
    let now_ms = chrono::Utc::now().timestamp_millis();

    let parse = |line: &str| -> Option<SyncDeployment> {
        let trimmed = line.trim();
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            if !(trimmed.is_empty() || trimmed.starts_with("###"))
                && num_parse_errors.fetch_add(1, Ordering::Relaxed) < 5
            {
                warn!(snapshot_hash, "Skipping unreadable snapshot line");
            }
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
        if !usable_snapshot_line(&deployment, now_ms) {
            if num_parse_errors.fetch_add(1, Ordering::Relaxed) < 5 {
                warn!(
                    snapshot_hash,
                    "Skipping snapshot line with implausible timestamp"
                );
            }
            return None;
        }
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

    let num_schedule_errors = AtomicU64::new(0);

    let server_list_ref: &[String] = &server_list;
    let scheduled = &num_scheduled;
    let schedule_errors = &num_schedule_errors;
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
                .schedule_entity_deployment(deployment, server_list_ref, Some(report))
                .await
            {
                Ok(()) => {
                    scheduled.fetch_add(1, Ordering::Relaxed);
                }
                Err(SyncError::Stopped) => stop_flag.store(true, Ordering::Relaxed),
                Err(e) => {
                    schedule_errors.fetch_add(1, Ordering::Relaxed);
                    warn!(snapshot_hash, error = %e, "Failed to schedule entity deployment");
                }
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
                .schedule_entity_deployment(deployment, server_list_ref, Some(report))
                .await
            {
                Ok(()) => {
                    scheduled.fetch_add(1, Ordering::Relaxed);
                }
                Err(SyncError::Stopped) => stop_flag.store(true, Ordering::Relaxed),
                Err(e) => {
                    schedule_errors.fetch_add(1, Ordering::Relaxed);
                    warn!(snapshot_hash, error = %e, "Failed to schedule entity deployment");
                }
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
    let num_schedule_errors = num_schedule_errors.load(Ordering::Relaxed);

    info!(
        snapshot_hash,
        total,
        num_scheduled,
        num_skipped_by_filter,
        num_parse_errors,
        num_schedule_errors,
        nonscene_concurrency,
        scene_concurrency,
        "Snapshot scheduled"
    );

    if num_parse_errors > 0 || num_schedule_errors > 0 {
        return Err(SyncError::Other(format!(
            "snapshot {} was not fully deployable: {} unreadable lines, {} schedule errors \
             (scheduled {})",
            snapshot_hash, num_parse_errors, num_schedule_errors, num_scheduled
        )));
    }

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
                    let content_encoding = response_content_encoding(&resp);

                    let buf = match read_body_capped(resp, MAX_BODY_BYTES).await {
                        Ok(buf) => buf,
                        Err(err) => {
                            warn!(
                                snapshot_hash,
                                %server,
                                retry,
                                %err,
                                "Snapshot body exceeded cap or failed to stream, trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "snapshot {} from {}: {}",
                                snapshot_hash, server, err
                            )));
                            if retry + 1 < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
                                    .await;
                            }
                            continue;
                        }
                    };

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

                    let decoded = match decode_content_encoding(
                        content_encoding.as_deref(),
                        buf,
                        MAX_BODY_BYTES,
                    ) {
                        Ok(decoded) => decoded,
                        Err(reason) => {
                            warn!(
                                snapshot_hash,
                                %server,
                                retry,
                                %reason,
                                "Snapshot body transfer decoding failed, trying next server"
                            );
                            last_error = Some(SyncError::Other(format!(
                                "snapshot {} from {}: {}",
                                snapshot_hash, server, reason
                            )));
                            if retry + 1 < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(retry_wait_ms))
                                    .await;
                            }
                            continue;
                        }
                    };
                    let bytes: bytes::Bytes = decoded.into();

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

    const NOW_MS: Timestamp = 1_750_000_000_000;
    const GOOD_HASH: &str = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
    const GOOD_HASH_2: &str = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdH";
    const GOOD_HASH_3: &str = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdJ";

    fn entry(hash: &str, init: i64, end: i64) -> serde_json::Value {
        serde_json::json!({
            "hash": hash,
            "timeRange": { "initTimestamp": init, "endTimestamp": end },
            "numberOfEntities": 10,
            "generationTimestamp": end,
        })
    }

    #[test]
    fn far_future_and_inverted_entries_are_discarded_and_counted() {
        let items = vec![
            entry(GOOD_HASH, 0, 1_700_000_000_000),
            entry(GOOD_HASH_2, 0, 253_402_300_799_000),
            entry(GOOD_HASH_3, 1_700_000_000_000, 1_600_000_000_000),
        ];
        let result = parse_snapshots_response(&items, NOW_MS, "https://peer.test");
        assert_eq!(result.discarded, 2);
        assert_eq!(result.snapshots.len(), 1);
        assert_eq!(result.snapshots[0].hash, GOOD_HASH);
    }

    #[test]
    fn valid_entry_parses_with_replaced_hashes_and_sorts_newest_first() {
        let mut older = entry(GOOD_HASH, 0, 1_600_000_000_000);
        older["replacedSnapshotHashes"] = serde_json::json!([GOOD_HASH_2]);
        let newer = entry(GOOD_HASH_3, 0, 1_700_000_000_000);
        let result = parse_snapshots_response(&[older, newer], NOW_MS, "s");
        assert_eq!(result.discarded, 0);
        assert_eq!(result.snapshots[0].hash, GOOD_HASH_3, "newest first");
        assert_eq!(
            result.snapshots[1].replaced_snapshot_hashes.as_deref(),
            Some(&[GOOD_HASH_2.to_string()][..])
        );
    }

    #[test]
    fn unread_fields_never_reject_an_entry() {
        let mut e = entry(GOOD_HASH, 0, 1_700_000_000_000);
        e["numberOfEntities"] = serde_json::json!("5");
        e["generationTimestamp"] = serde_json::json!("not a number");
        let parsed = parse_snapshot_metadata(&e, NOW_MS).expect("entry must survive");
        assert_eq!(parsed.number_of_entities, 0);
        assert_eq!(parsed.generation_timestamp, 0);
    }

    #[test]
    fn malformed_hash_or_replaced_hashes_reject_the_entry() {
        let traversal = entry("../../etc/passwd", 0, 1_700_000_000_000);
        assert!(parse_snapshot_metadata(&traversal, NOW_MS).is_none());

        let mut bad_replaced = entry(GOOD_HASH, 0, 1_700_000_000_000);
        bad_replaced["replacedSnapshotHashes"] = serde_json::json!(["not-a-cid"]);
        assert!(parse_snapshot_metadata(&bad_replaced, NOW_MS).is_none());

        let mut wrong_type = entry(GOOD_HASH, 0, 1_700_000_000_000);
        wrong_type["replacedSnapshotHashes"] = serde_json::json!("QmSingle");
        assert!(parse_snapshot_metadata(&wrong_type, NOW_MS).is_none());

        let mut fractional = entry(GOOD_HASH, 0, 1_700_000_000_000);
        fractional["timeRange"]["endTimestamp"] = serde_json::json!(1.7e12 + 0.5);
        assert!(
            parse_snapshot_metadata(&fractional, NOW_MS).is_none(),
            "epoch milliseconds are integers; a fraction only ever indicates a malformed server"
        );
    }

    #[test]
    fn snapshot_line_timestamps_get_the_plausibility_gate() {
        let line = |entity_ts: &str, local_ts: Option<&str>| {
            let local = local_ts.map_or(String::new(), |ts| format!(",\"localTimestamp\":{ts}"));
            format!(
                "{{\"entityId\":\"QmLine\",\"entityType\":\"scene\",\"pointers\":[\"0,0\"],\"authChain\":[],\"entityTimestamp\":{entity_ts}{local}}}"
            )
        };
        let parse = |s: &str| serde_json::from_str::<SyncDeployment>(s).unwrap();
        let now_ms = 1_700_000_000_000;

        assert!(usable_snapshot_line(
            &parse(&line("1600000000000", None)),
            now_ms
        ));
        assert!(
            !usable_snapshot_line(&parse(&line("253402300800000", None)), now_ms),
            "year-9999 entityTimestamp would win overwrite ordering forever"
        );
        assert!(
            !usable_snapshot_line(
                &parse(&line("1600000000000", Some("253402300800000"))),
                now_ms
            ),
            "an implausible localTimestamp is rejected even with a sane entityTimestamp"
        );
    }

    #[test]
    fn replacement_chain_collapses_in_a_single_call() {
        let h0 = "h0".to_string();
        let h1 = "h1".to_string();
        let h2 = "h2".to_string();
        let genesis = 0;
        let end_ts = 1_700_000_000_000;
        let candidates = vec![
            (h2.clone(), end_ts, vec![vec![h1.clone()]]),
            (h1.clone(), end_ts, vec![vec![h0.clone()]]),
        ];
        let mut processed: HashSet<String> = [h0].into_iter().collect();
        let (to_deploy, newly_marked) = decide_snapshot_pass(&candidates, &mut processed, genesis);
        assert!(
            to_deploy.is_empty(),
            "no link of a fully-replaced chain may be deployed, got {to_deploy:?}"
        );
        let mut sorted = newly_marked;
        sorted.sort();
        assert_eq!(sorted, vec![h1, h2], "both chain links must end up marked");
        assert!(processed.contains("h1") && processed.contains("h2"));
    }

    #[test]
    fn unreplaced_snapshot_deploys_and_genesis_cutoff_skips() {
        let candidates = vec![
            ("new".to_string(), 1_700_000_000_000, vec![]),
            ("ancient".to_string(), 5, vec![]),
        ];
        let mut processed = HashSet::new();
        let (to_deploy, marked) =
            decide_snapshot_pass(&candidates, &mut processed, 1_000_000_000_000);
        assert_eq!(to_deploy, vec!["new".to_string()]);
        assert!(marked.is_empty());
    }

    #[tokio::test]
    async fn gzip_labeled_snapshot_decodes_then_hashes_and_stores_decoded() {
        const PAYLOAD: &[u8] = b"### Decentraland json snapshot\n{\"entityId\":\"Qm1\"}\n";
        let cid = catalyrst_hashing::hash_bytes_v1(PAYLOAD);
        let wire = crate::sync::content_encoding::gzip(PAYLOAD);
        let (base, server) =
            crate::sync::test_support::spawn_content_server(wire, Some("gzip")).await;
        let (storage, tmp) = crate::sync::test_support::temp_content_storage("snap-gzip").await;
        let servers: HashSet<String> = [base].into_iter().collect();

        download_snapshot_file(&Client::new(), &storage, &cid, &servers, 1, 0)
            .await
            .expect("a gzip-labeled snapshot whose decoded bytes match the CID must be accepted");

        let stored = storage.retrieve(&cid).await.unwrap().unwrap();
        assert_eq!(
            &stored[..],
            PAYLOAD,
            "the stored bytes must be the decoded representation"
        );

        server.abort();
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn corrupt_snapshot_body_still_fails_hash_verification() {
        const PAYLOAD: &[u8] = b"### Decentraland json snapshot\n{\"entityId\":\"Qm1\"}\n";
        const OTHER: &[u8] = b"### Decentraland json snapshot\n{\"entityId\":\"Qm2\"}\n";
        let cid = catalyrst_hashing::hash_bytes_v1(PAYLOAD);
        let wire = crate::sync::content_encoding::gzip(OTHER);
        let (base, server) =
            crate::sync::test_support::spawn_content_server(wire, Some("gzip")).await;
        let (storage, tmp) = crate::sync::test_support::temp_content_storage("snap-corrupt").await;
        let servers: HashSet<String> = [base].into_iter().collect();

        let err = download_snapshot_file(&Client::new(), &storage, &cid, &servers, 1, 0)
            .await
            .expect_err("decoded bytes that mismatch the CID must still be rejected");
        assert!(err.to_string().contains("snapshot hash mismatch"), "{err}");
        assert!(!storage.exist(&cid).await.unwrap());

        server.abort();
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[test]
    fn any_advertisers_processed_group_suffices() {
        let mut processed: HashSet<String> =
            ["a".to_string(), "b".to_string()].into_iter().collect();
        let decision = decide_snapshot_deployment_from_processed_set(
            &mut processed,
            0,
            "snap",
            1_700_000_000_000,
            &[
                vec!["a".to_string(), "x".to_string()],
                vec!["a".to_string(), "b".to_string()],
            ],
        );
        assert_eq!(decision, SnapshotDecision::MarkProcessed);
        assert!(
            processed.contains("snap"),
            "mark must mutate the caller's set"
        );

        let mut processed = HashSet::new();
        let decision = decide_snapshot_deployment_from_processed_set(
            &mut processed,
            0,
            "snap2",
            1_700_000_000_000,
            &[vec![]],
        );
        assert_eq!(decision, SnapshotDecision::Deploy);
    }
}
