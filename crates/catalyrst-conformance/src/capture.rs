use anyhow::{Context, Result};
use base64::Engine;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use crate::fixture::{Fixture, RecordedRequest, RecordedResponse};
use crate::retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
use crate::{Ctx, Outcome, PairOutcome};

pub(crate) async fn capture_single(
    ctx: &Ctx,
    label: &str,
    baseline_url: &str,
    body: Option<&Value>,
) -> Result<PairOutcome> {
    let outcome = retry_with_backoff(label, 3, 1000, || async {
        let req = if let Some(b) = body {
            ctx.client.post(baseline_url).json(b)
        } else {
            ctx.client.get(baseline_url)
        };
        let resp = req
            .send()
            .await
            .with_context(|| format!("request {} (capture)", baseline_url))?;
        let status = resp.status();
        if is_transient_status(status) {
            let hint = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(hint));
        }
        let headers = collect_response_headers(&resp);
        let bytes = resp.bytes().await.context("reading capture body")?;
        Ok(RetryDecision::Done((status, headers, bytes)))
    })
    .await?;

    let (status, headers, bytes) = match outcome {
        Some(t) => t,
        None => {
            return Ok(PairOutcome::Transient(
                "baseline kept returning 429/5xx after 3 attempts (capture mode)".to_string(),
            ));
        }
    };

    let text = String::from_utf8_lossy(&bytes).to_string();
    write_fixture(
        ctx,
        label,
        if body.is_some() { "POST" } else { "GET" },
        baseline_url,
        body,
        status.as_u16(),
        &headers,
        &bytes,
    )?;
    Ok(PairOutcome::Got((status, status, text.clone(), text)))
}

fn collect_response_headers(resp: &reqwest::Response) -> BTreeMap<String, String> {
    let keep = [
        "content-type",
        "content-length",
        "etag",
        "cache-control",
        "last-modified",
    ];
    let mut out = BTreeMap::new();
    for name in keep {
        if let Some(v) = resp.headers().get(name).and_then(|v| v.to_str().ok()) {
            out.insert(name.to_string(), v.to_string());
        }
    }
    out
}

fn url_to_path_and_query(url: &str) -> String {
    if let Some(idx) = url.find("://") {
        let after_scheme = &url[idx + 3..];
        if let Some(slash) = after_scheme.find('/') {
            return after_scheme[slash..].to_string();
        }
    }
    url.to_string()
}

fn split_path_query(s: &str) -> (String, BTreeMap<String, Vec<String>>) {
    let mut q: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(idx) = s.find('?') {
        for kv in s[idx + 1..].split('&') {
            if kv.is_empty() {
                continue;
            }
            let mut it = kv.splitn(2, '=');
            let k = it.next().unwrap_or("").to_string();
            let v = it.next().unwrap_or("").to_string();
            q.entry(k).or_default().push(v);
        }
        (s[..idx].to_string(), q)
    } else {
        (s.to_string(), q)
    }
}

fn fixture_slug_for(method: &str, path_and_query: &str, body: Option<&Value>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    method.hash(&mut hasher);
    path_and_query.hash(&mut hasher);
    if let Some(b) = body {
        b.to_string().hash(&mut hasher);
    }
    let h = hasher.finish() & 0xffffffff;
    let (path, _) = split_path_query(path_and_query);
    let slug: String = path
        .trim_start_matches('/')
        .chars()
        .take(60)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("call-{:08x}", h)
    } else {
        format!("{}-{:08x}", slug, h)
    }
}

fn fixture_subdir_for(section: &str) -> &'static str {
    match section {
        "lambdas-status"
        | "contracts"
        | "third-party-integrations"
        | "collections"
        | "nfts-collections"
        | "profiles"
        | "user-items"
        | "collections-by-owner"
        | "explorer"
        | "parcel"
        | "name-owner"
        | "outfits" => "lambdas",

        _ => "content",
    }
}

pub(crate) async fn capture_bytes(
    ctx: &Ctx,
    section: &str,
    label: &str,
    method: &str,
    baseline_full: &str,
) -> Result<Outcome> {
    let attempted = retry_with_backoff(label, 3, 1000, || async {
        let resp = ctx
            .client
            .get(baseline_full)
            .send()
            .await
            .with_context(|| format!("GET {} (capture-bytes)", baseline_full))?;
        let status = resp.status();
        if is_transient_status(status) {
            let hint = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Ok(RetryDecision::Retry(hint));
        }
        let headers = collect_response_headers(&resp);
        let bytes = resp.bytes().await.context("reading capture body")?;
        Ok(RetryDecision::Done((status, headers, bytes)))
    })
    .await?;

    let (status, headers, bytes) = match attempted {
        Some(t) => t,
        None => {
            return Ok(Outcome::TransientSkip(format!(
                "{} drained retries (capture mode)",
                label
            )));
        }
    };

    write_fixture(
        ctx,
        section,
        method,
        baseline_full,
        None,
        status.as_u16(),
        &headers,
        &bytes,
    )?;
    Ok(Outcome::Diffs(Vec::new()))
}

#[allow(clippy::too_many_arguments)]
fn write_fixture(
    ctx: &Ctx,
    section: &str,
    method: &str,
    full_url: &str,
    body: Option<&Value>,
    status: u16,
    headers: &BTreeMap<String, String>,
    response_bytes: &[u8],
) -> Result<()> {
    let dir = ctx
        .capture_dir
        .as_ref()
        .expect("capture mode required for write_fixture");
    let pathq = url_to_path_and_query(full_url);
    let (path_only, query) = split_path_query(&pathq);

    let content_type = headers.get("content-type").cloned().unwrap_or_default();
    let (body_json, body_bytes_b64) = if content_type.contains("application/json")
        || response_bytes
            .first()
            .map(|c| *c == b'{' || *c == b'[')
            .unwrap_or(false)
    {
        match serde_json::from_slice::<Value>(response_bytes) {
            Ok(v) => (Some(v), None),
            Err(_) => (
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(response_bytes)),
            ),
        }
    } else {
        (
            None,
            Some(base64::engine::general_purpose::STANDARD.encode(response_bytes)),
        )
    };

    let fixture = Fixture {
        description: format!("Captured from baseline: {} {}", method, path_only),
        request: RecordedRequest {
            method: method.to_string(),
            path: path_only.clone(),
            query,
            headers: BTreeMap::new(),
            body: body.cloned(),
        },
        response: RecordedResponse {
            status,
            headers: headers.clone(),
            body_json,
            body_bytes_b64,
        },
        captured_from: ctx.captured_from.clone(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        volatile_paths: Vec::new(),
    };

    let subdir = dir.join(fixture_subdir_for(section));
    std::fs::create_dir_all(&subdir)
        .with_context(|| format!("creating fixture dir {}", subdir.display()))?;
    let slug = fixture_slug_for(method, &pathq, body);
    let outpath = subdir.join(format!("{}.json", slug));
    let json_str = serde_json::to_string_pretty(&fixture).context("serialising fixture")?;
    std::fs::write(&outpath, json_str)
        .with_context(|| format!("writing fixture {}", outpath.display()))?;

    ctx.capture_count.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
