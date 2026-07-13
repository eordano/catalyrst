use anyhow::{Context, Result};
use serde_json::Value;

use crate::retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
use crate::{BootstrapData, Ctx};

pub(crate) async fn bootstrap_data(ctx: &Ctx, baseline_content: &str) -> Result<BootstrapData> {
    let mut profile_entity_ids = Vec::new();
    let mut profile_addresses = Vec::new();
    let mut scene_entity_ids = Vec::new();
    let mut scene_pointers = Vec::new();
    let mut wearable_entity_ids = Vec::new();
    let mut content_hashes = Vec::new();

    let scene_url = format!(
        "{}/deployments?entityType=scene&limit=5&sortingOrder=DESC&fields=pointers,content,entityId,entityType",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-scenes", &scene_url).await? {
        if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
            for dep in deployments {
                if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                    scene_entity_ids.push(eid.to_string());
                }
                if let Some(ptrs) = dep.get("pointers").and_then(|v| v.as_array()) {
                    for p in ptrs {
                        if let Some(s) = p.as_str() {
                            scene_pointers.push(s.to_string());
                        }
                    }
                }
                if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                    for c in content {
                        if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                            content_hashes.push(hash.to_string());
                        }
                    }
                }
            }
        }
    }

    let profile_url = format!(
        "{}/deployments?entityType=profile&limit=5&sortingOrder=DESC&fields=pointers,content,entityId,entityType",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-profiles", &profile_url).await? {
        harvest_deployments(
            &body,
            &mut profile_entity_ids,
            &mut profile_addresses,
            &mut content_hashes,
        );
    }

    let wearable_url = format!(
        "{}/deployments?entityType=wearable&limit=5&sortingOrder=DESC&fields=entityId,content",
        baseline_content
    );
    if let Some(body) = fetch_json_with_retry(ctx, "bootstrap-wearables", &wearable_url).await? {
        if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
            for dep in deployments {
                if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                    wearable_entity_ids.push(eid.to_string());
                }
                if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                    for c in content {
                        if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                            content_hashes.push(hash.to_string());
                        }
                    }
                }
            }
        }
    }

    profile_entity_ids.sort();
    profile_entity_ids.dedup();
    profile_addresses.sort();
    profile_addresses.dedup();
    scene_entity_ids.sort();
    scene_entity_ids.dedup();
    scene_pointers.sort();
    scene_pointers.dedup();
    wearable_entity_ids.sort();
    wearable_entity_ids.dedup();
    content_hashes.sort();
    content_hashes.dedup();

    profile_entity_ids.truncate(5);
    profile_addresses.truncate(5);
    scene_entity_ids.truncate(5);
    scene_pointers.truncate(5);
    wearable_entity_ids.truncate(5);
    content_hashes.truncate(10);

    Ok(BootstrapData {
        profile_entity_ids,
        profile_addresses,
        scene_pointers,
        scene_entity_ids,
        wearable_entity_ids,
        content_hashes,
    })
}

async fn fetch_json_with_retry(ctx: &Ctx, label: &str, url: &str) -> Result<Option<Value>> {
    retry_with_backoff(label, 3, 1000, || async {
        let resp = ctx
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("GET {}", url))?;
        let status = resp.status();
        if is_transient_status(status) {
            let wait = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(wait));
        }
        if !status.is_success() {
            return Ok(RetryDecision::Done(None));
        }
        let body = resp.text().await.context("reading bootstrap body")?;
        let json: Value = serde_json::from_str(&body)
            .with_context(|| format!("parsing bootstrap JSON from {}", url))?;
        Ok(RetryDecision::Done(Some(json)))
    })
    .await
    .map(|opt| opt.flatten())
}

fn harvest_deployments(
    body: &Value,
    entity_ids: &mut Vec<String>,
    addresses: &mut Vec<String>,
    content_hashes: &mut Vec<String>,
) {
    if let Some(deployments) = body.get("deployments").and_then(|v| v.as_array()) {
        for dep in deployments {
            if let Some(eid) = dep.get("entityId").and_then(|v| v.as_str()) {
                entity_ids.push(eid.to_string());
            }
            if let Some(ptrs) = dep.get("pointers").and_then(|v| v.as_array()) {
                for p in ptrs {
                    if let Some(s) = p.as_str() {
                        if s.starts_with("0x") && s.len() == 42 {
                            addresses.push(s.to_string());
                        }
                    }
                }
            }
            if let Some(content) = dep.get("content").and_then(|v| v.as_array()) {
                for c in content {
                    if let Some(hash) = c.get("hash").and_then(|v| v.as_str()) {
                        content_hashes.push(hash.to_string());
                    }
                }
            }
        }
    }
}
