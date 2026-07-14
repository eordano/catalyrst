use anyhow::{Context, Result};
use serde_json::Value;

use crate::capture::{capture_bytes, capture_single};
use crate::diff::{compare_json, Difference};
use crate::retry::{is_transient_status, parse_retry_after, retry_with_backoff, RetryDecision};
use crate::{Ctx, Outcome, PairOutcome, Scoreboard};

pub(crate) async fn test_get_json(
    ctx: &Ctx,
    section: &str,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
) -> Result<Outcome> {
    if ctx.volatility.ignore_whole(section) {
        return Ok(Outcome::VolatilitySkip);
    }

    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    let pair = retry_pair(ctx, section, &baseline_full, &candidate_full, None).await?;
    let (b_status, c_status, b_body, c_body) = match pair {
        PairOutcome::Got(s) => s,
        PairOutcome::Transient(reason) => return Ok(Outcome::TransientSkip(reason)),
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: "HTTP status".to_string(),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    if b_body.trim().is_empty() && c_body.trim().is_empty() {
        return Ok(Outcome::Diffs(diffs));
    }
    if b_body.trim().is_empty() || c_body.trim().is_empty() {
        diffs.push(Difference {
            path: "body-presence".to_string(),
            baseline_value: format!("{} bytes", b_body.len()),
            candidate_value: format!("{} bytes", c_body.len()),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let b_json: Value = match serde_json::from_str(&b_body) {
        Ok(v) => v,
        Err(_) => {
            if b_body == c_body {
                return Ok(Outcome::Diffs(diffs));
            }
            diffs.push(Difference {
                path: "non-JSON-body".to_string(),
                baseline_value: truncate(&b_body),
                candidate_value: truncate(&c_body),
            });
            return Ok(Outcome::Diffs(diffs));
        }
    };
    let c_json: Value =
        serde_json::from_str(&c_body).context(format!("parsing candidate JSON from {}", path))?;

    diffs.extend(compare_json(
        section,
        path,
        &b_json,
        &c_json,
        &ctx.volatility,
    ));
    Ok(Outcome::Diffs(diffs))
}

pub(crate) async fn test_post_json(
    ctx: &Ctx,
    section: &str,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
    body: &Value,
) -> Result<Outcome> {
    if ctx.volatility.ignore_whole(section) {
        return Ok(Outcome::VolatilitySkip);
    }

    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    let pair = retry_pair(ctx, section, &baseline_full, &candidate_full, Some(body)).await?;
    let (b_status, c_status, b_body, c_body) = match pair {
        PairOutcome::Got(s) => s,
        PairOutcome::Transient(reason) => return Ok(Outcome::TransientSkip(reason)),
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: "HTTP status".to_string(),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let b_json: Value = serde_json::from_str(&b_body)
        .context(format!("parsing baseline JSON from POST {}", path))?;
    let c_json: Value = serde_json::from_str(&c_body)
        .context(format!("parsing candidate JSON from POST {}", path))?;

    diffs.extend(compare_json(
        section,
        path,
        &b_json,
        &c_json,
        &ctx.volatility,
    ));
    Ok(Outcome::Diffs(diffs))
}

async fn retry_pair(
    ctx: &Ctx,
    label: &str,
    baseline_url: &str,
    candidate_url: &str,
    body: Option<&Value>,
) -> Result<PairOutcome> {
    if ctx.is_capturing() {
        return capture_single(ctx, label, baseline_url, body).await;
    }
    let outcome = retry_with_backoff(label, 3, 1000, || async {
        let send_one = |url: &str| {
            let req = if let Some(b) = body {
                ctx.client.post(url).json(b)
            } else {
                ctx.client.get(url)
            };
            req.send()
        };

        let (b_resp, c_resp) = tokio::try_join!(
            async {
                send_one(baseline_url)
                    .await
                    .with_context(|| format!("request {} (baseline)", baseline_url))
            },
            async {
                send_one(candidate_url)
                    .await
                    .with_context(|| format!("request {} (candidate)", candidate_url))
            },
        )?;

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            let rate_limited = b_status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || c_status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            return Ok(RetryDecision::Retry {
                after: hint,
                rate_limited,
            });
        }

        let (b_body, c_body) = tokio::try_join!(
            async { b_resp.text().await.context("reading baseline body") },
            async { c_resp.text().await.context("reading candidate body") },
        )?;

        Ok(RetryDecision::Done((b_status, c_status, b_body, c_body)))
    })
    .await?;

    match outcome {
        Some(tup) => Ok(PairOutcome::Got(tup)),
        None => Ok(PairOutcome::Transient(
            "baseline/candidate kept returning 429/5xx after exhausting retries".to_string(),
        )),
    }
}

pub(crate) async fn test_pagination(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    score: &mut Scoreboard,
    verbose: bool,
) -> Result<()> {
    let section = "deployments";
    let initial_path = "/deployments?entityType=profile&limit=5&sortingOrder=DESC";

    let mut b_next: Option<String> = Some(format!("{}{}", baseline_base, initial_path));
    let mut c_next: Option<String> = Some(format!("{}{}", candidate_base, initial_path));

    for page in 1..=3 {
        // The baseline peer rate-limits /content/deployments with a tiny
        // burst budget; pace page fetches so we don't drain it.
        ctx.sleep_heavy().await;
        let b_url = match &b_next {
            Some(u) => u.clone(),
            None => {
                score.record(
                    &[Difference {
                        path: "pagination".to_string(),
                        baseline_value: "no next link".to_string(),
                        candidate_value: "n/a".to_string(),
                    }],
                    &format!("Page {}: baseline has no next link", page),
                    verbose,
                );
                return Ok(());
            }
        };
        let c_url = match &c_next {
            Some(u) => u.clone(),
            None => {
                score.record(
                    &[Difference {
                        path: "pagination".to_string(),
                        baseline_value: "n/a".to_string(),
                        candidate_value: "no next link".to_string(),
                    }],
                    &format!("Page {}: candidate has no next link", page),
                    verbose,
                );
                return Ok(());
            }
        };

        let pair = retry_pair(
            ctx,
            &format!("pagination-page-{}", page),
            &b_url,
            &c_url,
            None,
        )
        .await?;
        let (b_status, c_status, b_text, c_text) = match pair {
            PairOutcome::Got(s) => s,
            PairOutcome::Transient(reason) => {
                score.record_outcome(
                    Outcome::TransientSkip(reason),
                    &format!("Page {} (pagination)", page),
                    verbose,
                );
                return Ok(());
            }
        };

        if !b_status.is_success() || !c_status.is_success() {
            if b_status != c_status {
                score.record(
                    &[Difference {
                        path: "HTTP status".to_string(),
                        baseline_value: b_status.to_string(),
                        candidate_value: c_status.to_string(),
                    }],
                    &format!("Page {}: non-2xx response", page),
                    verbose,
                );
            } else {
                // Both peers agree on the same non-2xx status: that is a match,
                // not a divergence (e.g. both 404 past the last page).
                score.record(
                    &[],
                    &format!("Page {}: both non-2xx ({})", page, b_status),
                    verbose,
                );
            }
            return Ok(());
        }

        let b_body: Value = match serde_json::from_str(&b_text) {
            Ok(v) => v,
            Err(e) => {
                score.record(
                    &[Difference {
                        path: "baseline JSON parse".to_string(),
                        baseline_value: format!("{}: {}", e, truncate(&b_text)),
                        candidate_value: format!("{} bytes", c_text.len()),
                    }],
                    &format!("Page {}: baseline body is not JSON", page),
                    verbose,
                );
                return Ok(());
            }
        };
        let c_body: Value = match serde_json::from_str(&c_text) {
            Ok(v) => v,
            Err(e) => {
                score.record(
                    &[Difference {
                        path: "candidate JSON parse".to_string(),
                        baseline_value: format!("{} bytes", b_text.len()),
                        candidate_value: format!("{}: {}", e, truncate(&c_text)),
                    }],
                    &format!("Page {}: candidate body is not JSON", page),
                    verbose,
                );
                return Ok(());
            }
        };

        let page_label = format!("deployments_page_{}", page);
        let diffs = compare_json(section, &page_label, &b_body, &c_body, &ctx.volatility);

        let b_count = b_body
            .get("deployments")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let deployment_diffs: Vec<&Difference> = diffs
            .iter()
            .filter(|d| d.path.starts_with(&format!("{}.deployments", page_label)))
            .collect();
        let match_count = if deployment_diffs.is_empty() {
            b_count
        } else {
            let diffed_indices: std::collections::HashSet<usize> = deployment_diffs
                .iter()
                .filter_map(|d| {
                    let s = &d.path;
                    let bracket = s.find('[')?;
                    let end = s[bracket..].find(']')?;
                    s[bracket + 1..bracket + end].parse::<usize>().ok()
                })
                .collect();
            b_count.saturating_sub(diffed_indices.len())
        };

        score.record(
            &diffs,
            &format!("Page {}: {}/{} match", page, match_count, b_count),
            verbose,
        );

        b_next = extract_next_link(&b_body, baseline_base);
        c_next = extract_next_link(&c_body, candidate_base);
    }

    Ok(())
}

fn extract_next_link(body: &Value, base_url: &str) -> Option<String> {
    let next = body
        .get("pagination")
        .and_then(|p| p.get("next"))
        .and_then(|n| n.as_str())?;

    if next.is_empty() {
        return None;
    }

    if next.starts_with("http") {
        Some(next.to_string())
    } else if next.starts_with('/') {
        let origin = base_url
            .rfind("/content")
            .map(|i| &base_url[..i])
            .unwrap_or(base_url);
        Some(format!("{}{}", origin, next))
    } else if next.starts_with('?') {
        Some(format!("{}/deployments{}", base_url, next))
    } else {
        Some(format!("{}/{}", base_url, next))
    }
}

pub(crate) async fn test_content_hash(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    hash: &str,
) -> Result<Outcome> {
    let baseline_full = format!("{}/contents/{}", baseline_base, hash);
    let candidate_full = format!("{}/contents/{}", candidate_base, hash);
    let label = format!("contents/{}", hash);

    if ctx.is_capturing() {
        return capture_bytes(ctx, "content", &label, "GET", &baseline_full).await;
    }

    let attempted = retry_with_backoff(&label, 3, 1000, || async {
        let (b_resp, c_resp) = tokio::try_join!(
            async {
                ctx.client
                    .get(&baseline_full)
                    .send()
                    .await
                    .with_context(|| format!("GET content {} (baseline)", hash))
            },
            async {
                ctx.client
                    .get(&candidate_full)
                    .send()
                    .await
                    .with_context(|| format!("GET content {} (candidate)", hash))
            },
        )?;

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            let rate_limited = b_status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || c_status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            return Ok(RetryDecision::Retry {
                after: hint,
                rate_limited,
            });
        }

        Ok(RetryDecision::Done((b_resp, c_resp, b_status, c_status)))
    })
    .await?;

    let (b_resp, c_resp, b_status, c_status) = match attempted {
        Some(t) => t,
        None => {
            return Ok(Outcome::TransientSkip(format!(
                "content hash {}... drained retries",
                &hash[..hash.len().min(12)]
            )))
        }
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: format!("contents/{} HTTP status", hash),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let header_path = |name: &str| format!("contents/{} {}", hash, name);

    let read_header = |resp: &reqwest::Response, name: &str| -> String {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };

    let b_ct = read_header(&b_resp, "content-type");
    let c_ct = read_header(&c_resp, "content-type");
    let b_cl = read_header(&b_resp, "content-length");
    let c_cl = read_header(&c_resp, "content-length");
    let b_etag = read_header(&b_resp, "etag");
    let c_etag = read_header(&c_resp, "etag");

    let (b_bytes, c_bytes) = tokio::try_join!(
        async {
            b_resp
                .bytes()
                .await
                .context("reading baseline content bytes")
        },
        async {
            c_resp
                .bytes()
                .await
                .context("reading candidate content bytes")
        },
    )?;

    if b_ct != c_ct {
        diffs.push(Difference {
            path: header_path("Content-Type"),
            baseline_value: b_ct,
            candidate_value: c_ct,
        });
    }
    if b_cl != c_cl {
        diffs.push(Difference {
            path: header_path("Content-Length"),
            baseline_value: b_cl,
            candidate_value: c_cl,
        });
    }
    if !b_etag.is_empty() && !c_etag.is_empty() && b_etag != c_etag {
        diffs.push(Difference {
            path: header_path("ETag"),
            baseline_value: b_etag,
            candidate_value: c_etag,
        });
    }
    if b_bytes != c_bytes {
        diffs.push(Difference {
            path: header_path("body"),
            baseline_value: format!("{} bytes", b_bytes.len()),
            candidate_value: format!("{} bytes", c_bytes.len()),
        });
    }

    Ok(Outcome::Diffs(diffs))
}

pub(crate) async fn test_get_bytes(
    ctx: &Ctx,
    baseline_base: &str,
    candidate_base: &str,
    path: &str,
) -> Result<Outcome> {
    let baseline_full = format!("{}{}", baseline_base, path);
    let candidate_full = format!("{}{}", candidate_base, path);

    if ctx.is_capturing() {
        return capture_bytes(ctx, "content", path, "GET", &baseline_full).await;
    }

    let attempted = retry_with_backoff(path, 3, 1000, || async {
        let (b_resp, c_resp) = tokio::try_join!(
            async {
                ctx.client
                    .get(&baseline_full)
                    .send()
                    .await
                    .with_context(|| format!("GET {} (baseline)", baseline_full))
            },
            async {
                ctx.client
                    .get(&candidate_full)
                    .send()
                    .await
                    .with_context(|| format!("GET {} (candidate)", candidate_full))
            },
        )?;

        let b_status = b_resp.status();
        let c_status = c_resp.status();

        if is_transient_status(b_status) || is_transient_status(c_status) {
            let hint = [&b_resp, &c_resp]
                .iter()
                .filter_map(|r| {
                    r.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                })
                .max();
            let rate_limited = b_status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || c_status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            return Ok(RetryDecision::Retry {
                after: hint,
                rate_limited,
            });
        }

        Ok(RetryDecision::Done((b_resp, c_resp, b_status, c_status)))
    })
    .await?;

    let (b_resp, c_resp, b_status, c_status) = match attempted {
        Some(t) => t,
        None => return Ok(Outcome::TransientSkip(format!("{} drained retries", path))),
    };

    let mut diffs = Vec::new();

    if b_status != c_status {
        diffs.push(Difference {
            path: format!("{} HTTP status", path),
            baseline_value: b_status.to_string(),
            candidate_value: c_status.to_string(),
        });
        return Ok(Outcome::Diffs(diffs));
    }

    let (b_bytes, c_bytes) = tokio::try_join!(
        async { b_resp.bytes().await.context("reading baseline bytes") },
        async { c_resp.bytes().await.context("reading candidate bytes") },
    )?;

    if b_bytes != c_bytes {
        diffs.push(Difference {
            path: format!("{} body", path),
            baseline_value: format!("{} bytes", b_bytes.len()),
            candidate_value: format!("{} bytes", c_bytes.len()),
        });
    }

    Ok(Outcome::Diffs(diffs))
}

fn truncate(s: &str) -> String {
    if s.len() > 120 {
        format!("{}...", &s[..117])
    } else {
        s.to_string()
    }
}
