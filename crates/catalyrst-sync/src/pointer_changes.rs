use std::collections::HashSet;
use reqwest::Client;
use tracing::{debug, info, warn};

use crate::{SyncDeployment, SyncError, Timestamp};

#[derive(Debug, Clone)]
pub struct PointerChangesOptions {
    pub from_timestamp: Timestamp,
    pub wait_time_ms: u64,
}

#[derive(Debug, serde::Deserialize)]
struct PointerChangesPage {
    deltas: Vec<serde_json::Value>,
    #[serde(default)]
    pagination: Option<PaginationInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct PaginationInfo {
    next: Option<String>,
}

async fn fetch_page(
    client: &Client,
    url: &str,
) -> Result<(Vec<serde_json::Value>, Option<String>), SyncError> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let page: PointerChangesPage = resp.json().await?;
    let next_url = page.pagination.and_then(|p| p.next);
    Ok((page.deltas, next_url))
}

fn resolve_url(server: &str, maybe_relative: &str) -> Result<Option<String>, SyncError> {
    let base = url::Url::parse(server).map_err(|e| {
        SyncError::Other(format!("invalid server URL '{}': {}", server, e))
    })?;
    match url::Url::parse(maybe_relative) {
        Ok(absolute) => {

            if absolute.scheme() != base.scheme() || absolute.host_str() != base.host_str() {
                warn!(
                    server = %server,
                    next = %maybe_relative,
                    "Rejecting cross-host pagination.next URL"
                );
                return Ok(None);
            }
            Ok(Some(absolute.to_string()))
        }
        Err(_) => {

            match base.join(maybe_relative) {
                Ok(resolved) => {
                    if resolved.host_str() != base.host_str() {
                        warn!(
                            server = %server,
                            next = %maybe_relative,
                            "Rejecting cross-host resolved next URL"
                        );
                        return Ok(None);
                    }
                    Ok(Some(resolved.to_string()))
                }
                Err(e) => Err(SyncError::Other(format!(
                    "failed to resolve next URL '{}' against '{}': {}",
                    maybe_relative, server, e
                ))),
            }
        }
    }
}

pub async fn deploy_entities_from_pointer_changes<D, S>(
    client: &Client,
    server: &str,
    options: &PointerChangesOptions,
    deployer: &D,
    content_servers: &[String],
    entity_type_filter: Option<&HashSet<String>>,
    should_stop: S,
) -> Result<Timestamp, SyncError>
where
    D: crate::batch_deployer::DeploymentScheduler,
    S: Fn() -> bool,
{
    info!(
        server,
        from_timestamp = options.from_timestamp,
        has_type_filter = entity_type_filter.is_some(),
        "Starting pointer-changes stream"
    );

    let mut greatest_timestamp = options.from_timestamp;
    let mut url = format!(
        "{}/pointer-changes?sortingOrder=ASC&sortingField=local_timestamp&from={}",
        server, options.from_timestamp
    );

    // Pipeline the cursor: keep the NEXT page's download in flight (spawned) while
    // we drain the current page into the concurrent deploy scheduler. A single
    // source's stream is otherwise page-serial — fetch, deploy, fetch, deploy —
    // which leaves an idle box waiting on each per-page round-trip. Overlapping the
    // fetch with the deploys keeps the scheduler fed. reqwest::Client is an Arc, so
    // the clone is cheap.
    let spawn_fetch = |u: String| {
        let c = client.clone();
        tokio::spawn(async move { fetch_page(&c, &u).await })
    };

    let mut in_flight = Some(spawn_fetch(url.clone()));

    loop {
        if should_stop() {
            if let Some(h) = in_flight.take() { h.abort(); }
            return Ok(greatest_timestamp);
        }

        let (items, next_url) = match in_flight.take() {
            Some(h) => h
                .await
                .map_err(|e| SyncError::Other(format!("pointer-changes prefetch join: {e}")))??,
            None => fetch_page(client, &url).await?,
        };

        if items.is_empty() {
            debug!(server, from = greatest_timestamp, "No new pointer-changes");
        }

        let resolved_next = match next_url {
            Some(next) => resolve_url(&url, &next)?,
            None => None,
        };

        // Start the next page download BEFORE deploying this page's items so the
        // network round-trip overlaps the (concurrent) deploys below.
        if let Some(next) = &resolved_next {
            in_flight = Some(spawn_fetch(next.clone()));
            url = next.clone();
        }

        for item in items {
            if should_stop() {
                if let Some(h) = in_flight.take() { h.abort(); }
                return Ok(greatest_timestamp);
            }

            let deployment: SyncDeployment = match serde_json::from_value(item) {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "Invalid deployment from /pointer-changes, skipping");
                    continue;
                }
            };

            if let Some(local_ts) = deployment.local_timestamp {
                if local_ts >= options.from_timestamp {
                    greatest_timestamp = greatest_timestamp.max(local_ts);
                }
            }

            if let Some(filter) = entity_type_filter {
                if !filter.contains(&deployment.entity_type) {
                    continue;
                }
            }

            deployer
                .schedule_entity_deployment(deployment, content_servers)
                .await?;
        }

        // No cursor advance: end of stream (or live-tail). in_flight is None here.
        if resolved_next.is_none() {
            if options.wait_time_ms == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(options.wait_time_ms)).await;
            url = format!(
                "{}/pointer-changes?sortingOrder=ASC&sortingField=local_timestamp&from={}",
                server, greatest_timestamp
            );
            in_flight = Some(spawn_fetch(url.clone()));
        }
    }

    info!(server, greatest_timestamp, "Pointer-changes stream ended");
    Ok(greatest_timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url_absolute() {

        let result = resolve_url(
            "https://peer.example.com/content",
            "https://peer.example.com/content/pointer-changes?from=42",
        )
        .expect("resolve_url should not fail on a same-host absolute URL");
        assert_eq!(
            result.as_deref(),
            Some("https://peer.example.com/content/pointer-changes?from=42")
        );
    }

    #[test]
    fn test_resolve_url_rejects_cross_host_next() {

        let result = resolve_url(
            "https://peer.example.com/content",
            "https://other.example.com/foo",
        )
        .expect("resolve_url should not error on a parseable cross-host URL");
        assert_eq!(result, None, "cross-host next must be dropped");

        let scheme_pivot = resolve_url(
            "https://peer.example.com/content",
            "http://peer.example.com/content/pointer-changes",
        )
        .expect("resolve_url should not error on a scheme-pivot URL");
        assert_eq!(scheme_pivot, None, "scheme pivot must be dropped");
    }

    #[test]
    fn test_resolve_url_relative() {
        let result = resolve_url(
            "https://peer.example.com/content",
            "/content/pointer-changes?from=123",
        )
        .expect("relative resolve should succeed");
        assert_eq!(
            result.as_deref(),
            Some("https://peer.example.com/content/pointer-changes?from=123")
        );
    }

    #[test]
    fn test_resolve_url_query_only_keeps_path() {
        let current = "https://peer.example.com/content/pointer-changes?from=0";
        let next = "?from=100&to=200&limit=500&lastId=abc";
        let result = resolve_url(current, next).expect("query-only resolve should succeed");
        assert_eq!(
            result.as_deref(),
            Some("https://peer.example.com/content/pointer-changes?from=100&to=200&limit=500&lastId=abc")
        );
    }
}
