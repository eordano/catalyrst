use reqwest::Client;
use std::collections::HashSet;
use tracing::{debug, info, warn};

use super::backends::LiveDeploymentRepository;
use super::batch_deployer::{BatchDeployer, DeploymentReport};
use super::{SyncDeployment, SyncError, Timestamp};

#[derive(Debug, Clone)]
pub struct PointerChangesOptions {
    pub from_timestamp: Timestamp,
    pub wait_time_ms: u64,
}

/// Upstream MAX_TIMESTAMP_CLOCK_SKEW_IN_MS: how far ahead of our own clock a remote timestamp
/// may sit and still be adopted as sync state.
pub(crate) const MAX_TIMESTAMP_CLOCK_SKEW_MS: Timestamp = 24 * 60 * 60 * 1000;

/// Bound on rejected-delta log lines per stream, as upstream's MAX_REJECTED_DELTA_LOGS: a server
/// can put a rejectable delta on every page, so per-item logging is attacker-sized.
const MAX_REJECTED_DELTA_LOGS: u64 = 100;

/// Upstream MAX_BOUNDARY_ROWS_TRACKED. Ours only stops *tracking* past the cap (falling back to
/// the DB-side dedup, wasted work but never wrong) where upstream fails the poll, because for us
/// the suppression is purely an optimization: identity below is entity-level, and the deployer
/// already dedups entities durably.
const MAX_BOUNDARY_ROWS_TRACKED: usize = 10_000;

/// A remote timestamp we are willing to adopt as sync state (upstream `isUsableTimestamp`).
///
/// These values become the stream's high-water mark, and the persisted resume state (the
/// global frontier and the server's own cursor) only ever moves forward
/// (`advance_sync_frontier` / `advance_server_sync_cursor` are GREATEST-monotonic), so a
/// single bad one is permanent:
/// the node then polls /pointer-changes from a point no real deployment can exceed and silently
/// stops syncing. `localTimestamp` is exactly what the poll boundary feeds to the frontier,
/// which is why this is checked before the tentative mark ever sees the value. The JS
/// safe-integer clause is a non-concern here: i64 covers every value serde hands us, and
/// anything that saturated on the way in (1e999 -> i64::MAX) fails the upper bound.
pub(crate) fn is_usable_timestamp(ts: Timestamp, now_ms: Timestamp) -> bool {
    ts >= 0 && ts <= now_ms.saturating_add(MAX_TIMESTAMP_CLOCK_SKEW_MS)
}

/// Validates one raw /pointer-changes delta and, only when it is acceptable, folds its
/// `localTimestamp` into the tentative high-water mark. Returns the deployment to schedule, or
/// None when the delta was rejected -- and a rejected delta never moves the mark, which is the
/// upstream-53e9c07 guarantee this function exists to keep in one place: the mark becomes the
/// confirmed timestamp at the next poll boundary and from there the durable, GREATEST-monotonic
/// frontier, so one hostile delta reaching the max() below would poison the node permanently.
///
/// Rejection skips the single delta rather than failing the stream, matching upstream's
/// deliberate trade: a delta is one entity, and failing on it would let a single
/// permanently-broken record stall every later deployment from that server.
fn accept_delta_and_advance_mark(
    item: serde_json::Value,
    from_timestamp: Timestamp,
    now_ms: Timestamp,
    greatest_timestamp: &mut Timestamp,
    rejected_logged: &mut u64,
) -> Option<SyncDeployment> {
    let mut report_rejected = |reason: &str, detail: String| {
        if *rejected_logged < MAX_REJECTED_DELTA_LOGS {
            *rejected_logged += 1;
            warn!(reason, detail, "Rejecting delta from /pointer-changes");
            if *rejected_logged == MAX_REJECTED_DELTA_LOGS {
                warn!(
                    suppressed_after = MAX_REJECTED_DELTA_LOGS,
                    "Too many rejected deltas from /pointer-changes, suppressing further logs"
                );
            }
        }
    };

    let deployment: SyncDeployment = match serde_json::from_value(item) {
        Ok(d) => d,
        Err(e) => {
            report_rejected("unparseable", e.to_string());
            return None;
        }
    };

    let entity_ts_ok = is_usable_timestamp(deployment.entity_timestamp, now_ms);
    let local_ts_ok = deployment
        .local_timestamp
        .is_none_or(|ts| is_usable_timestamp(ts, now_ms));
    if !entity_ts_ok || !local_ts_ok {
        report_rejected(
            "implausible timestamp",
            format!(
                "entity_id={} entityTimestamp={} localTimestamp={:?}",
                deployment.entity_id, deployment.entity_timestamp, deployment.local_timestamp
            ),
        );
        return None;
    }

    if let Some(local_ts) = deployment.local_timestamp {
        if local_ts >= from_timestamp {
            // Tentative only: committed to `progress` at the next confirmed boundary.
            *greatest_timestamp = (*greatest_timestamp).max(local_ts);
        }
    }

    Some(deployment)
}

/// Suppresses re-delivery of boundary rows across polls (a set-keyed port of upstream's
/// `boundaryRowFingerprint` map). `from=` is inclusive and each poll restarts from the
/// high-water timestamp, so every poll re-returns the rows sitting exactly there; without this,
/// each of them costs an `is_entity_deployed` round trip per poll, forever. Entity-level
/// identity (entity_id + entity_timestamp + local_timestamp) is sufficient for our path -- the
/// deployer dedups by entity_id anyway -- so unlike upstream this never affects correctness,
/// only wasted work.
struct BoundaryTracker {
    /// The timestamp the tracked rows sit at: the stream's current high-water mark.
    ts: Timestamp,
    /// Rows delivered at `ts`, accumulated while the mark stays put.
    delivered: HashSet<(String, Timestamp, Timestamp)>,
    /// This poll's suppression budget: what earlier polls delivered at the poll's `from`.
    /// Frozen at `begin_poll` so a poll's own rows never suppress each other.
    suppress: HashSet<(String, Timestamp, Timestamp)>,
}

impl BoundaryTracker {
    fn new(from_timestamp: Timestamp) -> Self {
        Self {
            ts: from_timestamp,
            delivered: HashSet::new(),
            suppress: HashSet::new(),
        }
    }

    fn fingerprint(deployment: &SyncDeployment) -> Option<(String, Timestamp, Timestamp)> {
        deployment.local_timestamp.map(|local| {
            (
                deployment.entity_id.clone(),
                deployment.entity_timestamp,
                local,
            )
        })
    }

    /// Called at every poll boundary, before the next poll begins.
    fn begin_poll(&mut self) {
        self.suppress = self.delivered.clone();
    }

    /// Whether an earlier poll already delivered this row at the current boundary. Also advances
    /// the tracked timestamp: when the mark moves, nothing has been delivered at the new one yet.
    fn already_delivered(&mut self, deployment: &SyncDeployment) -> bool {
        let Some(fp) = Self::fingerprint(deployment) else {
            return false;
        };
        if fp.2 > self.ts {
            self.ts = fp.2;
            self.delivered.clear();
            return false;
        }
        // Spend one allowance per matching row, so a genuinely new identical-identity row in a
        // later poll is not suppressed.
        fp.2 == self.ts && self.suppress.remove(&fp)
    }

    /// Records a row this poll delivered at the current high-water timestamp.
    fn record_delivered(&mut self, deployment: &SyncDeployment) {
        let Some(fp) = Self::fingerprint(deployment) else {
            return;
        };
        if fp.2 == self.ts && self.delivered.len() < MAX_BOUNDARY_ROWS_TRACKED {
            self.delivered.insert(fp);
        }
    }
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
    let base = url::Url::parse(server)
        .map_err(|e| SyncError::Other(format!("invalid server URL '{}': {}", server, e)))?;
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
        Err(_) => match base.join(maybe_relative) {
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
        },
    }
}

/// Streams /pointer-changes into the deployer.
///
/// The resume cursor (`progress`, the return value, and the persisted frontier) only ever
/// reflects CONFIRMED progress: the stream holds its high-water timestamp tentative while
/// entities are merely scheduled, and commits it at each poll boundary -- the end of a
/// pagination chain, the only checkpoint a stream designed to keep polling has -- after the
/// deployer has drained and every deployment scheduled by this stream has been acknowledged
/// (deployed, or durably recorded in failed_deployments). A boundary where something remains
/// unacknowledged fails the stream instead, so the caller reconnects from the last confirmed
/// timestamp and the missing entities are re-delivered. Committing per entity, as this used
/// to, let a later confirmed entity carry the cursor past an earlier one that was still in
/// flight -- a crash in that window skipped it forever.
pub async fn deploy_entities_from_pointer_changes<S>(
    client: &Client,
    server: &str,
    options: &PointerChangesOptions,
    deployer: &BatchDeployer,
    content_servers: &[String],
    entity_type_filter: Option<&HashSet<String>>,
    heartbeat_repo: Option<std::sync::Arc<LiveDeploymentRepository>>,
    report: &std::sync::Arc<DeploymentReport>,
    progress: &std::sync::atomic::AtomicI64,
    should_stop: S,
) -> Result<Timestamp, SyncError>
where
    S: Fn() -> bool,
{
    let mut last_beat: Option<std::time::Instant> = None;
    let mut last_persisted = options.from_timestamp;
    info!(
        server,
        from_timestamp = options.from_timestamp,
        has_type_filter = entity_type_filter.is_some(),
        "Starting pointer-changes stream"
    );

    let mut greatest_timestamp = options.from_timestamp;
    let mut confirmed_timestamp = options.from_timestamp;
    let mut rejected_logged: u64 = 0;
    let mut tracker = BoundaryTracker::new(options.from_timestamp);
    let mut url = format!(
        "{}/pointer-changes?sortingOrder=ASC&sortingField=local_timestamp&from={}",
        server, options.from_timestamp
    );

    let spawn_fetch = |u: String| {
        let c = client.clone();
        tokio::spawn(async move { fetch_page(&c, &u).await })
    };

    let mut in_flight = Some(spawn_fetch(url.clone()));

    loop {
        if should_stop() {
            if let Some(h) = in_flight.take() {
                h.abort();
            }
            return Ok(confirmed_timestamp);
        }

        let (items, next_url) = match in_flight.take() {
            Some(h) => h
                .await
                .map_err(|e| SyncError::Other(format!("pointer-changes prefetch join: {e}")))??,
            None => fetch_page(client, &url).await?,
        };

        if let Some(repo) = &heartbeat_repo {
            if last_beat.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(10)) {
                let _ = repo
                    .set_sync_heartbeat(chrono::Utc::now().timestamp_millis())
                    .await;
                last_beat = Some(std::time::Instant::now());
            }
        }

        if items.is_empty() {
            debug!(server, from = greatest_timestamp, "No new pointer-changes");
        }

        let resolved_next = match next_url {
            Some(next) => resolve_url(&url, &next)?,
            None => None,
        };

        if let Some(next) = &resolved_next {
            in_flight = Some(spawn_fetch(next.clone()));
            url = next.clone();
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        for item in items {
            if should_stop() {
                if let Some(h) = in_flight.take() {
                    h.abort();
                }
                return Ok(confirmed_timestamp);
            }

            // Validation and the tentative-mark update live in one function so a rejected
            // delta provably cannot move the mark (see accept_delta_and_advance_mark).
            let Some(deployment) = accept_delta_and_advance_mark(
                item,
                options.from_timestamp,
                now_ms,
                &mut greatest_timestamp,
                &mut rejected_logged,
            ) else {
                continue;
            };

            // `from=` is inclusive: skip the boundary rows an earlier poll already delivered.
            if tracker.already_delivered(&deployment) {
                continue;
            }

            if let Some(filter) = entity_type_filter {
                if !filter.contains(&deployment.entity_type) {
                    continue;
                }
            }

            // Recorded before the handoff: if scheduling fails the whole stream fails and the
            // tracker dies with it, so an over-record cannot outlive the poll it belongs to.
            tracker.record_delivered(&deployment);
            deployer
                .schedule_entity_deployment(deployment, content_servers, Some(report))
                .await?;
        }

        if resolved_next.is_none() {
            // Poll boundary: drain the deployer, verify everything this stream scheduled came
            // back acknowledged, and only then commit the tentative high-water mark. The drain
            // sits at the end of a pagination chain rather than per page, so it only ever
            // waits for the residual queue.
            deployer.on_idle().await?;
            if !report.is_complete() {
                return Err(SyncError::Other(format!(
                    "pointer-changes deployments for {} not fully acknowledged ({} of {}); \
                     reconnecting from the last confirmed timestamp",
                    server,
                    report.acknowledged(),
                    report.scheduled()
                )));
            }
            if report.lost() > 0 {
                // Losses are attributed per report (the batch flush carries each entity's
                // report), so only the stream that actually lost an entity holds back --
                // unrelated concurrent streams commit their own boundaries undisturbed.
                return Err(SyncError::Other(format!(
                    "deployer reported silently lost entities while syncing {}; \
                     reconnecting from the last confirmed timestamp",
                    server
                )));
            }
            confirmed_timestamp = greatest_timestamp;
            progress.fetch_max(confirmed_timestamp, std::sync::atomic::Ordering::Relaxed);
            if let Some(repo) = &heartbeat_repo {
                if confirmed_timestamp > last_persisted {
                    let _ = repo.advance_sync_frontier(confirmed_timestamp).await;
                    // The same confirmed boundary, recorded for THIS server only: the
                    // per-server cursor is what bootstrap resumes from, so it must never
                    // reflect another server's progress the way the global frontier does.
                    let _ = repo
                        .advance_server_sync_cursor(server, confirmed_timestamp)
                        .await;
                    last_persisted = confirmed_timestamp;
                }
            }

            if options.wait_time_ms == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(options.wait_time_ms)).await;
            // The next poll restarts from the (inclusive) high-water timestamp; freeze what this
            // poll delivered there as its suppression budget.
            tracker.begin_poll();
            url = format!(
                "{}/pointer-changes?sortingOrder=ASC&sortingField=local_timestamp&from={}",
                server, greatest_timestamp
            );
            in_flight = Some(spawn_fetch(url.clone()));
        }
    }

    info!(server, confirmed_timestamp, "Pointer-changes stream ended");
    Ok(confirmed_timestamp)
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

    fn delta(local_timestamp: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "entityId": "QmPoisonPoisonPoisonPoisonPoisonPoisonPoison1",
            "entityType": "scene",
            "pointers": ["0,0"],
            "authChain": [],
            "entityTimestamp": 1_700_000_000_000i64,
            "localTimestamp": local_timestamp,
        })
    }

    const NOW_MS: i64 = 1_750_000_000_000;
    const FROM_TS: i64 = 1_600_000_000_000;

    // The upstream-53e9c07 poisoning scenario: one delta with an impossible localTimestamp must
    // not move the tentative high-water mark, because the mark becomes the confirmed timestamp
    // at the poll boundary and from there the durable GREATEST-monotonic frontier -- a poisoned
    // value there is permanent and silently kills the whole node's sync.
    #[test]
    fn far_future_local_timestamp_is_rejected_and_never_moves_the_mark() {
        let mut mark = FROM_TS;
        let mut logged = 0;
        // Year 9999: a plain integer, so serde accepts it without complaint.
        let rejected = accept_delta_and_advance_mark(
            delta(serde_json::json!(253_402_300_799_000i64)),
            FROM_TS,
            NOW_MS,
            &mut mark,
            &mut logged,
        );
        assert!(rejected.is_none(), "far-future delta must be rejected");
        assert_eq!(mark, FROM_TS, "a rejected delta must not move the mark");

        let negative = accept_delta_and_advance_mark(
            delta(serde_json::json!(-1)),
            FROM_TS,
            NOW_MS,
            &mut mark,
            &mut logged,
        );
        assert!(negative.is_none(), "negative delta must be rejected");
        assert_eq!(mark, FROM_TS);
    }

    #[test]
    fn json_1e999_local_timestamp_cannot_poison_the_mark() {
        // Without serde_json's arbitrary_precision feature "1e999" fails to parse at all, which
        // fails the page fetch (safe: the stream errors and reconnects, nothing commits). If a
        // parse mode ever starts admitting it, the saturating i64 cast lands on i64::MAX, which
        // the usability guard must reject before the mark update.
        let raw = r#"{
            "entityId": "QmPoisonPoisonPoisonPoisonPoisonPoisonPoison1",
            "entityType": "scene",
            "pointers": ["0,0"],
            "authChain": [],
            "entityTimestamp": 1700000000000,
            "localTimestamp": 1e999
        }"#;
        let Ok(item) = serde_json::from_str::<serde_json::Value>(raw) else {
            return; // rejected at parse time: the poisoned value never reaches the stream
        };
        let mut mark = FROM_TS;
        let mut logged = 0;
        let rejected = accept_delta_and_advance_mark(item, FROM_TS, NOW_MS, &mut mark, &mut logged);
        assert!(rejected.is_none(), "overflowed timestamp must be rejected");
        assert_eq!(mark, FROM_TS, "a rejected delta must not move the mark");
    }

    #[test]
    fn implausible_entity_timestamp_rejects_the_delta_too() {
        // entityTimestamp feeds the deployed-entity dedup probe and the deployments row itself;
        // upstream guards both fields together.
        let mut item = delta(serde_json::json!(FROM_TS + 5));
        item["entityTimestamp"] = serde_json::json!(253_402_300_799_000i64);
        let mut mark = FROM_TS;
        let mut logged = 0;
        let rejected = accept_delta_and_advance_mark(item, FROM_TS, NOW_MS, &mut mark, &mut logged);
        assert!(rejected.is_none());
        assert_eq!(mark, FROM_TS);
    }

    #[test]
    fn plausible_delta_is_accepted_and_moves_the_mark() {
        let mut mark = FROM_TS;
        let mut logged = 0;
        let accepted = accept_delta_and_advance_mark(
            delta(serde_json::json!(FROM_TS + 1000)),
            FROM_TS,
            NOW_MS,
            &mut mark,
            &mut logged,
        );
        assert!(accepted.is_some());
        assert_eq!(mark, FROM_TS + 1000);
        assert_eq!(logged, 0);

        // Within the allowed clock skew: still acceptable.
        let skewed = accept_delta_and_advance_mark(
            delta(serde_json::json!(NOW_MS + MAX_TIMESTAMP_CLOCK_SKEW_MS - 1)),
            FROM_TS,
            NOW_MS,
            &mut mark,
            &mut logged,
        );
        assert!(skewed.is_some());
        assert_eq!(mark, NOW_MS + MAX_TIMESTAMP_CLOCK_SKEW_MS - 1);
    }

    #[test]
    fn delta_without_local_timestamp_is_scheduled_but_never_moves_the_mark() {
        let mut item = delta(serde_json::json!(0));
        item.as_object_mut().unwrap().remove("localTimestamp");
        let mut mark = FROM_TS;
        let mut logged = 0;
        let accepted = accept_delta_and_advance_mark(item, FROM_TS, NOW_MS, &mut mark, &mut logged);
        assert!(accepted.is_some());
        assert_eq!(mark, FROM_TS);
    }

    fn deployment_at(entity_id: &str, local_ts: Timestamp) -> SyncDeployment {
        serde_json::from_value(serde_json::json!({
            "entityId": entity_id,
            "entityType": "scene",
            "pointers": ["0,0"],
            "authChain": [],
            "entityTimestamp": 1_700_000_000_000i64,
            "localTimestamp": local_ts,
        }))
        .expect("test deployment must deserialize")
    }

    #[test]
    fn boundary_tracker_suppresses_rows_redelivered_at_the_inclusive_boundary() {
        let mut tracker = BoundaryTracker::new(100);

        // Poll 1 delivers two rows at the high-water timestamp 200.
        let a = deployment_at("QmA", 200);
        let b = deployment_at("QmB", 200);
        assert!(!tracker.already_delivered(&a));
        tracker.record_delivered(&a);
        assert!(!tracker.already_delivered(&b));
        tracker.record_delivered(&b);

        // Poll 2 restarts from the inclusive from=200 and re-serves both, plus a new row there.
        tracker.begin_poll();
        assert!(
            tracker.already_delivered(&a),
            "replayed row must be skipped"
        );
        assert!(
            tracker.already_delivered(&b),
            "replayed row must be skipped"
        );
        let c = deployment_at("QmC", 200);
        assert!(!tracker.already_delivered(&c), "new boundary row must pass");
        tracker.record_delivered(&c);

        // Poll 3: everything delivered at 200 so far is budget; the mark then advances, which
        // resets tracking -- and rows at the OLD boundary no longer match the new timestamp.
        tracker.begin_poll();
        assert!(tracker.already_delivered(&c));
        let d = deployment_at("QmD", 300);
        assert!(!tracker.already_delivered(&d), "advancing row must pass");
        tracker.record_delivered(&d);
        tracker.begin_poll();
        assert!(
            tracker.already_delivered(&deployment_at("QmD", 300)),
            "row at the new boundary is suppressed on the next poll"
        );
        assert!(
            !tracker.already_delivered(&deployment_at("QmA", 200)),
            "rows below the new boundary are not suppression candidates"
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
