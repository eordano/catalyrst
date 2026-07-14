use anyhow::{Context, Result};
use serde_json::Value;

use crate::retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
use crate::{Args, BootstrapData, Ctx};

/// NFT-heavy account that exercises materially bigger profile / user-item
/// payloads than a freshly-deployed trivial profile. Pinned by default so
/// every run covers the heavy path (disable with --no-default-pins).
pub(crate) const DEFAULT_HEAVY_PROFILE_ADDRESS: &str =
    "0x29fb0d1b0836f9963f963b0bb07c49d2d61370b4";

/// Pinned values first (CLI order preserved, verbatim), then sampled values
/// not already present, truncated to `cap`. Pinned-first ordering is
/// load-bearing: sections.rs selects test subjects with `.first()`.
pub(crate) fn merge_pins(pinned: &[String], sampled: Vec<String>, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(cap);
    for p in pinned {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    for s in sampled {
        if out.len() >= cap {
            break;
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.truncate(cap);
    out
}

/// Profile-address pins: explicit --profile-address flags win; with no flags
/// the heavy default address is pinned unless --no-default-pins.
pub(crate) fn effective_profile_pins(args: &Args) -> Vec<String> {
    if args.profile_address.is_empty() && !args.no_default_pins {
        vec![DEFAULT_HEAVY_PROFILE_ADDRESS.to_string()]
    } else {
        args.profile_address.clone()
    }
}

pub(crate) async fn bootstrap_data(
    ctx: &Ctx,
    baseline_content: &str,
    args: &Args,
) -> Result<BootstrapData> {
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

    Ok(BootstrapData {
        profile_entity_ids: merge_pins(&args.profile_entity_id, profile_entity_ids, 5),
        profile_addresses: merge_pins(&effective_profile_pins(args), profile_addresses, 5),
        scene_pointers: merge_pins(&args.scene_pointer, scene_pointers, 5),
        scene_entity_ids: merge_pins(&args.scene_entity_id, scene_entity_ids, 5),
        wearable_entity_ids: merge_pins(&args.wearable_entity_id, wearable_entity_ids, 5),
        content_hashes: merge_pins(&args.content_hash, content_hashes, 10),
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
            return Ok(RetryDecision::Retry {
                after: wait,
                rate_limited: status == reqwest::StatusCode::TOO_MANY_REQUESTS,
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn merge_pins_pins_come_first_in_cli_order() {
        let out = merge_pins(&v(&["0xb", "0xa"]), v(&["0xc", "0xd"]), 5);
        assert_eq!(out, v(&["0xb", "0xa", "0xc", "0xd"]));
    }

    #[test]
    fn merge_pins_dedupes_overlap_between_pins_and_sampled() {
        let out = merge_pins(&v(&["0xa", "0xa"]), v(&["0xb", "0xa", "0xb"]), 5);
        assert_eq!(out, v(&["0xa", "0xb"]));
    }

    #[test]
    fn merge_pins_enforces_cap() {
        let out = merge_pins(&v(&["p1", "p2"]), v(&["s1", "s2", "s3"]), 3);
        assert_eq!(out, v(&["p1", "p2", "s1"]));

        // Pins alone beyond the cap are also truncated.
        let out = merge_pins(&v(&["p1", "p2", "p3"]), Vec::new(), 2);
        assert_eq!(out, v(&["p1", "p2"]));
    }

    #[test]
    fn merge_pins_empty_pins_is_sampled_passthrough() {
        let out = merge_pins(&[], v(&["s1", "s2"]), 5);
        assert_eq!(out, v(&["s1", "s2"]));
    }

    #[test]
    fn default_heavy_address_pinned_when_no_flags() {
        let args = Args::try_parse_from(["x"]).unwrap();
        assert_eq!(
            effective_profile_pins(&args),
            v(&[DEFAULT_HEAVY_PROFILE_ADDRESS])
        );

        let merged = merge_pins(&effective_profile_pins(&args), v(&["0xsampled"]), 5);
        assert_eq!(merged[0], DEFAULT_HEAVY_PROFILE_ADDRESS);
    }

    #[test]
    fn no_default_pins_yields_pure_sample() {
        let args = Args::try_parse_from(["x", "--no-default-pins"]).unwrap();
        assert!(effective_profile_pins(&args).is_empty());
        let merged = merge_pins(&effective_profile_pins(&args), v(&["0xsampled"]), 5);
        assert_eq!(merged, v(&["0xsampled"]));
    }

    #[test]
    fn explicit_profile_address_overrides_default() {
        let args = Args::try_parse_from(["x", "--profile-address", "0xexplicit"]).unwrap();
        assert_eq!(effective_profile_pins(&args), v(&["0xexplicit"]));
    }

    #[test]
    fn clap_collects_repeated_pin_flags_in_order() {
        let args = Args::try_parse_from([
            "x",
            "--profile-address",
            "0xa",
            "--profile-address",
            "0xb",
            "--scene-pointer",
            "0,0",
            "--scene-pointer",
            "-9,-9",
            "--profile-entity-id",
            "bafkrei1",
            "--scene-entity-id",
            "bafkrei2",
            "--wearable-entity-id",
            "bafkrei3",
            "--content-hash",
            "bafybei4",
        ])
        .unwrap();
        assert_eq!(args.profile_address, v(&["0xa", "0xb"]));
        assert_eq!(args.scene_pointer, v(&["0,0", "-9,-9"]));
        assert_eq!(args.profile_entity_id, v(&["bafkrei1"]));
        assert_eq!(args.scene_entity_id, v(&["bafkrei2"]));
        assert_eq!(args.wearable_entity_id, v(&["bafkrei3"]));
        assert_eq!(args.content_hash, v(&["bafybei4"]));
        assert!(!args.no_default_pins);
    }
}
