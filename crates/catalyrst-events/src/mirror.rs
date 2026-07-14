use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

/// Upstream clamps `limit` to 500 (events getEventList.ts).
const PAGE: i64 = 500;
/// A cycle aborts after this many consecutive failed pages so an unreachable
/// upstream cannot spin the offset forever.
const MAX_CONSECUTIVE_PAGE_FAILURES: u32 = 3;
/// Hard offset ceiling; the live catalog is ~7.5k events, so hitting this
/// means the upstream is misbehaving, not that the catalog grew 30x.
const MAX_OFFSET: i64 = 250_000;
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; catalyrst-events-mirror/1; +https://decentraland.org)";

/// The conflict action skips rows carrying a local signed overlay
/// (events_local): node-local writes -- edits, soft-deletes -- take precedence
/// over the upstream copy, matching the sweep's exclusion rule.
const UPSERT: &str = r#"
    INSERT INTO event
        (id, name, start_at, finish_at, next_start_at, next_finish_at, duration_ms,
         recurrent, highlighted, trending, approved, attending, community_id,
         user_creator, coordinates_x, coordinates_y, description, raw, fetched_at)
    VALUES
        ($1, $2, $3::timestamptz, $4::timestamptz, $5::timestamptz, $6::timestamptz, $7,
         $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, now())
    ON CONFLICT (id) DO UPDATE SET
        name           = EXCLUDED.name,
        start_at       = EXCLUDED.start_at,
        finish_at      = EXCLUDED.finish_at,
        next_start_at  = EXCLUDED.next_start_at,
        next_finish_at = EXCLUDED.next_finish_at,
        duration_ms    = EXCLUDED.duration_ms,
        recurrent      = EXCLUDED.recurrent,
        highlighted    = EXCLUDED.highlighted,
        trending       = EXCLUDED.trending,
        approved       = EXCLUDED.approved,
        attending      = EXCLUDED.attending,
        community_id   = EXCLUDED.community_id,
        user_creator   = EXCLUDED.user_creator,
        coordinates_x  = EXCLUDED.coordinates_x,
        coordinates_y  = EXCLUDED.coordinates_y,
        description    = EXCLUDED.description,
        raw            = EXCLUDED.raw,
        fetched_at     = now()
    WHERE NOT EXISTS (SELECT 1 FROM events_local l WHERE l.id = event.id)
"#;

/// Rows the mirror no longer sees upstream are deleted only after two
/// consecutive full passes fail to list them (the cutoff sits two mirror
/// intervals before the current pass, so one pagination miss never deletes a
/// live row), except events carrying a local signed overlay (events_local) --
/// those are node-local writes the upstream never serves back.
const SWEEP: &str = r#"
    DELETE FROM event e
    WHERE e.fetched_at < $1
      AND NOT EXISTS (SELECT 1 FROM events_local l WHERE l.id = e.id)
"#;

/// The rows SWEEP would delete at a given cutoff, counted before deleting.
const SWEEP_CANDIDATES: &str = r#"
    SELECT count(*) FROM event e
    WHERE e.fetched_at < $1
      AND NOT EXISTS (SELECT 1 FROM events_local l WHERE l.id = e.id)
"#;

/// The mirror-owned rows the sweep can reach at all.
const MIRRORED_COUNT: &str = r#"
    SELECT count(*) FROM event e
    WHERE NOT EXISTS (SELECT 1 FROM events_local l WHERE l.id = e.id)
"#;

/// The sweep refuses to delete anything when its candidate set exceeds
/// max(SWEEP_FUSE_MIN_CANDIDATES, 20% of the mirror-owned rows): a
/// well-formed but truncated upstream response still reports complete, and
/// must not mass-delete the catalog.
const SWEEP_FUSE_MIN_CANDIDATES: i64 = 50;

#[derive(Debug, Default)]
pub struct CycleOutcome {
    pub upserted: usize,
    pub skipped_rows: usize,
    pub failed_rows: usize,
    pub failed_pages: usize,
    pub swept: u64,
    /// True only when every page fetched and every row upserted cleanly; the
    /// delete sweep is gated on it so a flaky cycle can never wipe rows it
    /// merely failed to see.
    pub complete: bool,
}

pub fn spawn(pool: PgPool, upstream_url: String, interval: Duration) {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "event mirror: http client build failed; disabled");
                return;
            }
        };
        loop {
            match run_cycle(&pool, &client, &upstream_url, interval).await {
                Ok(o) => tracing::info!(
                    upserted = o.upserted,
                    skipped_rows = o.skipped_rows,
                    failed_rows = o.failed_rows,
                    failed_pages = o.failed_pages,
                    swept = o.swept,
                    complete = o.complete,
                    "event catalog mirrored from upstream"
                ),
                Err(e) => tracing::warn!(error = %e, "event catalog mirror cycle failed"),
            }
            tokio::time::sleep(interval).await;
        }
    });
}

/// One full mirror pass: pages through the upstream public catalog
/// (`list=all`, which serves approved, non-rejected, non-deleted events --
/// past and future), upserts every row, then sweeps rows the upstream has
/// stopped serving. Fail-open per page and per row: an error logs, counts,
/// and the pass moves on.
pub async fn run_cycle(
    pool: &PgPool,
    client: &reqwest::Client,
    upstream: &str,
    interval: Duration,
) -> Result<CycleOutcome> {
    let base = upstream.trim_end_matches('/');
    let pass_start = Utc::now();
    let mut out = CycleOutcome {
        complete: true,
        ..Default::default()
    };
    let mut offset = 0i64;
    let mut consecutive_failures = 0u32;

    loop {
        if offset >= MAX_OFFSET {
            tracing::warn!(offset, "event mirror: offset ceiling reached; ending pass");
            out.complete = false;
            break;
        }
        let url = format!("{base}/api/events?list=all&limit={PAGE}&offset={offset}");
        match fetch_page(client, &url).await {
            Ok(events) => {
                consecutive_failures = 0;
                if events.is_empty() {
                    break;
                }
                let count = events.len() as i64;
                for event in &events {
                    match upsert_event(pool, event).await {
                        Ok(true) => out.upserted += 1,
                        Ok(false) => out.skipped_rows += 1,
                        Err(e) => {
                            out.failed_rows += 1;
                            out.complete = false;
                            let id = event.get("id").and_then(Value::as_str).unwrap_or("?");
                            tracing::warn!(error = %e, event_id = id, "event mirror: upsert failed; row skipped");
                        }
                    }
                }
                if count < PAGE {
                    break;
                }
                offset += PAGE;
            }
            Err(e) => {
                out.failed_pages += 1;
                out.complete = false;
                consecutive_failures += 1;
                tracing::warn!(error = %e, url = %url, "event mirror: page fetch failed; skipped");
                if consecutive_failures >= MAX_CONSECUTIVE_PAGE_FAILURES {
                    tracing::warn!(
                        consecutive_failures,
                        "event mirror: too many consecutive page failures; ending pass"
                    );
                    break;
                }
                offset += PAGE;
            }
        }
    }

    if out.complete && out.upserted > 0 {
        // A row is deleted only after two consecutive full passes fail to
        // list it: rows seen during pass N carry fetched_at >= that pass's
        // start, so a cutoff two intervals before this pass keeps anything
        // the previous pass listed -- one pagination miss never deletes.
        let grace = interval
            .checked_mul(2)
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(|| chrono::Duration::hours(2));
        let cutoff = pass_start - grace;
        match sweep_removed(pool, cutoff).await {
            Ok(SweepOutcome::Swept(n)) => out.swept = n,
            Ok(SweepOutcome::Refused { .. }) => {}
            Err(e) => {
                tracing::warn!(error = %e, "event mirror: sweep failed");
            }
        }
    }
    Ok(out)
}

async fn fetch_page(client: &reqwest::Client, url: &str) -> Result<Vec<Value>> {
    let body: Value = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_page(&body)
}

/// The upstream envelope is `{"ok": true, "data": [event, ...]}`.
pub fn parse_page(body: &Value) -> Result<Vec<Value>> {
    if body.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow!("events upstream returned ok!=true"));
    }
    body.get("data")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow!("events upstream response has no data array"))
}

/// The indexed columns the serving queries filter and order on; everything
/// else the read surface needs stays in `raw`. Every field except `id` is
/// tolerant of being absent or null upstream.
#[derive(Debug, PartialEq)]
pub struct EventFields<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub start_at: Option<&'a str>,
    pub finish_at: Option<&'a str>,
    pub next_start_at: Option<&'a str>,
    pub next_finish_at: Option<&'a str>,
    pub duration_ms: Option<i64>,
    pub recurrent: bool,
    pub highlighted: bool,
    pub trending: bool,
    pub approved: bool,
    pub attending: Option<bool>,
    pub community_id: Option<&'a str>,
    pub user: Option<&'a str>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub description: &'a str,
}

fn coord(event: &Value, idx: usize) -> Option<i32> {
    event
        .get("coordinates")
        .and_then(Value::as_array)
        .and_then(|a| a.get(idx))
        .and_then(Value::as_i64)
        .map(|v| v as i32)
}

/// None when the event has no usable id; such rows are skipped, not fatal.
pub fn extract_fields(event: &Value) -> Option<EventFields<'_>> {
    let id = match event.get("id").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => return None,
    };
    Some(EventFields {
        id,
        name: event.get("name").and_then(Value::as_str).unwrap_or(""),
        start_at: event.get("start_at").and_then(Value::as_str),
        finish_at: event.get("finish_at").and_then(Value::as_str),
        next_start_at: event.get("next_start_at").and_then(Value::as_str),
        next_finish_at: event.get("next_finish_at").and_then(Value::as_str),
        duration_ms: event.get("duration").and_then(Value::as_i64),
        recurrent: event
            .get("recurrent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        highlighted: event
            .get("highlighted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        trending: event
            .get("trending")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        approved: event
            .get("approved")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        attending: event.get("attending").and_then(Value::as_bool),
        community_id: event.get("community_id").and_then(Value::as_str),
        user: event.get("user").and_then(Value::as_str),
        x: coord(event, 0).or_else(|| event.get("x").and_then(Value::as_i64).map(|v| v as i32)),
        y: coord(event, 1).or_else(|| event.get("y").and_then(Value::as_i64).map(|v| v as i32)),
        description: event
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    })
}

/// Ok(false) means the row was skipped (no id), Ok(true) means upserted.
pub async fn upsert_event(pool: &PgPool, event: &Value) -> Result<bool> {
    let Some(f) = extract_fields(event) else {
        return Ok(false);
    };
    sqlx::query(UPSERT)
        .bind(f.id)
        .bind(f.name)
        .bind(f.start_at)
        .bind(f.finish_at)
        .bind(f.next_start_at)
        .bind(f.next_finish_at)
        .bind(f.duration_ms)
        .bind(f.recurrent)
        .bind(f.highlighted)
        .bind(f.trending)
        .bind(f.approved)
        .bind(f.attending)
        .bind(f.community_id)
        .bind(f.user)
        .bind(f.x)
        .bind(f.y)
        .bind(f.description)
        .bind(event)
        .execute(pool)
        .await?;
    Ok(true)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SweepOutcome {
    /// Rows deleted.
    Swept(u64),
    /// The volume fuse tripped: `candidates` rows were up for deletion out
    /// of `mirrored` mirror-owned rows, and nothing was deleted.
    Refused { candidates: i64, mirrored: i64 },
}

/// Deletes mirrored rows last seen before `cutoff`, sparing local writes.
/// Refuses to delete anything when the candidate set exceeds
/// max(SWEEP_FUSE_MIN_CANDIDATES, 20% of the mirror-owned rows).
pub async fn sweep_removed(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<SweepOutcome> {
    let candidates: i64 = sqlx::query_scalar(SWEEP_CANDIDATES)
        .bind(cutoff)
        .fetch_one(pool)
        .await?;
    let mirrored: i64 = sqlx::query_scalar(MIRRORED_COUNT).fetch_one(pool).await?;
    let threshold = SWEEP_FUSE_MIN_CANDIDATES.max(mirrored / 5);
    if candidates > threshold {
        tracing::warn!(
            candidates,
            mirrored,
            threshold,
            "event mirror: sweep would delete too many rows; skipping sweep"
        );
        return Ok(SweepOutcome::Refused {
            candidates,
            mirrored,
        });
    }
    let res = sqlx::query(SWEEP).bind(cutoff).execute(pool).await?;
    Ok(SweepOutcome::Swept(res.rows_affected()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real captured responses from events.decentraland.org -- the active
    /// default list and the first page of the full catalog (oldest events,
    /// which carry the most nulls).
    const ACTIVE_PAGE: &str = include_str!("../tests/fixtures/upstream_events_active_page0.json");
    const ALL_PAGE: &str = include_str!("../tests/fixtures/upstream_events_all_page0.json");

    fn events(fixture: &str) -> Vec<Value> {
        let body: Value = serde_json::from_str(fixture).expect("fixture is valid JSON");
        parse_page(&body).expect("fixture envelope parses")
    }

    #[test]
    fn active_fixture_parses_all_events() {
        let evs = events(ACTIVE_PAGE);
        assert_eq!(evs.len(), 100);
        for e in &evs {
            let f = extract_fields(e).expect("every live event has an id");
            assert!(!f.id.is_empty());
            assert!(!f.name.is_empty());
            assert!(f.start_at.is_some());
            assert!(f.finish_at.is_some());
            assert!(f.next_start_at.is_some());
            assert!(f.next_finish_at.is_some());
            assert!(f.duration_ms.is_some());
            assert!(f.approved);
            assert!(f.x.is_some() && f.y.is_some());
        }
    }

    #[test]
    fn all_catalog_fixture_parses_all_events() {
        let evs = events(ALL_PAGE);
        assert_eq!(evs.len(), 100);
        for e in &evs {
            assert!(extract_fields(e).is_some());
        }
        // Oldest events have community_id null everywhere -- nullable fields
        // must extract as None, not fail.
        assert!(evs
            .iter()
            .all(|e| extract_fields(e).unwrap().community_id.is_none()));
    }

    #[test]
    fn nullable_fields_extract_from_live_data() {
        let evs = events(ACTIVE_PAGE);
        let with_community = evs
            .iter()
            .filter(|e| extract_fields(e).unwrap().community_id.is_some())
            .count();
        let without_community = evs.len() - with_community;
        assert!(with_community > 0, "live page carries community events");
        assert!(without_community > 0, "live page carries null community_id");
    }

    #[test]
    fn coordinates_extract_as_ints() {
        let evs = events(ACTIVE_PAGE);
        let f = extract_fields(&evs[0]).unwrap();
        let coords = evs[0]["coordinates"].as_array().unwrap();
        assert_eq!(f.x, coords[0].as_i64().map(|v| v as i32));
        assert_eq!(f.y, coords[1].as_i64().map(|v| v as i32));
    }

    #[test]
    fn envelope_rejects_ok_false_and_missing_data() {
        assert!(parse_page(&serde_json::json!({"ok": false, "data": []})).is_err());
        assert!(parse_page(&serde_json::json!({"ok": true})).is_err());
        assert!(parse_page(&serde_json::json!({"data": []})).is_err());
    }

    #[test]
    fn event_without_id_is_skipped() {
        assert!(extract_fields(&serde_json::json!({"name": "no id"})).is_none());
        assert!(extract_fields(&serde_json::json!({"id": ""})).is_none());
    }
}
