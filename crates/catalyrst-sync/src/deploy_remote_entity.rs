use std::collections::HashSet;
use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use crate::{AuthChain, ContentStorage, Deployer, DeploymentContext, SyncError};

const MAX_DOWNLOAD_RETRIES: u32 = 3;
const RETRY_WAIT_MS: u64 = 500;

const MAX_BODY_BYTES: usize = 512 * 1024 * 1024;

pub async fn deploy_entity_streaming(
    client: &Client,
    storage: Arc<dyn ContentStorage>,
    deployer: &dyn Deployer,
    entity_id: &str,
    auth_chain: &AuthChain,
    servers: &[String],
    context: DeploymentContext,
    content_semaphore: Arc<Semaphore>,
) -> Result<(), SyncError> {
    download_file_with_retries(client, storage.as_ref(), entity_id, servers).await?;

    let entity_data =
        storage
            .retrieve(entity_id)
            .await?
            .ok_or_else(|| SyncError::EntityNotFound {
                entity_id: entity_id.to_string(),
            })?;

    let hashes = extract_content_hashes(&entity_data)?;

    let mut tasks: tokio::task::JoinSet<Result<(), SyncError>> = tokio::task::JoinSet::new();

    for hash in hashes {
        if storage.exists(&hash).await? {
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
        .deploy_entity(&entity_data, entity_id, auth_chain, context)
        .await?;

    Ok(())
}

fn extract_content_hashes(entity_data: &[u8]) -> Result<Vec<String>, SyncError> {
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
    storage: &dyn ContentStorage,
    hash: &str,
    servers: &[String],
) -> Result<(), SyncError> {
    if storage.exists(hash).await? {
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

                    // Explicit truncation signal (short clean EOF) instead of an
                    // opaque downstream hash-verification failure.
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

                    let bytes: bytes::Bytes = buf.into();
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
