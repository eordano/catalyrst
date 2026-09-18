//! Refresh of `marketplace.wearable_last_seen`, the "Suggested" sort's signal: for each wearable
//! URN, the newest `entity_timestamp` among the active profiles of the last 30 days wearing it.
//!
//! A full pass unnests every wearable of every active profile in the window (about 2.9 M entries,
//! three seconds of JSON parsing) and overwrites the table. Doing that every ten minutes was the
//! largest query consumer on the node, so between full passes the job scans only what the content
//! DB stored since the previous pass -- by `local_timestamp`, the content server's own clock, which
//! is monotonic where `entity_timestamp` (the deployer's clock) can arrive days late -- and merges
//! with GREATEST, which never lowers a value. The daily full pass restores the exact 30-day
//! semantics: a URN whose newest wearer has since redeployed without it comes back down, and a URN
//! nobody in the window wears keeps its last value, as it always did.

use std::time::Duration;

use anyhow::Result;
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(600);
/// A full pass at least this often; also the bound on how long a value that should have come
/// down can stay up.
pub const FULL_PASS_INTERVAL: TimeDelta = TimeDelta::hours(24);
/// Re-scan this much before the recorded clock, for a row whose `local_timestamp` was assigned
/// before the previous scan's snapshot but committed after it. The merge is idempotent, so the
/// overlap only costs the parse of an hour of profiles.
pub const SAFETY_MARGIN: TimeDelta = TimeDelta::hours(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshState {
    /// Content-DB clock (UTC, the `local_timestamp` domain) read just before the last scan.
    pub scanned_through: NaiveDateTime,
    pub last_full_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pass {
    Full,
    Incremental {
        since: NaiveDateTime,
        last_full_at: DateTime<Utc>,
    },
}

pub fn plan_pass(state: Option<&RefreshState>, now: DateTime<Utc>) -> Pass {
    match state {
        Some(s) if now - s.last_full_at < FULL_PASS_INTERVAL => Pass::Incremental {
            since: s.scanned_through - SAFETY_MARGIN,
            last_full_at: s.last_full_at,
        },
        _ => Pass::Full,
    }
}

pub fn next_state(pass: &Pass, scanned_through: NaiveDateTime, now: DateTime<Utc>) -> RefreshState {
    RefreshState {
        scanned_through,
        last_full_at: match pass {
            Pass::Full => now,
            Pass::Incremental { last_full_at, .. } => *last_full_at,
        },
    }
}

/// The inner aggregate takes the newest timestamp per raw URN first, so the normalisation runs
/// once per distinct URN rather than once per worn entry.
const SCAN_HEAD: &str =
    "SELECT lower(array_to_string((string_to_array(worn.urn, ':'))[1:6], ':')) AS urn,
       max(worn.last_seen) AS last_seen
FROM (
    SELECT w.urn, max(d.entity_timestamp) AS last_seen
    FROM deployments d,
         json_array_elements(d.entity_metadata->'v'->'avatars') a,
         json_array_elements_text(a->'avatar'->'wearables') w(urn)
    WHERE d.entity_type = 'profile' AND d.deleter_deployment IS NULL
      AND d.entity_timestamp > now() - interval '30 days'";

const SCAN_TAIL: &str = "    GROUP BY w.urn
) worn
GROUP BY 1";

/// The watermark is inlined, not bound: the generic plan for `local_timestamp > $1` is a parallel
/// bitmap scan of the whole 30-day window (7.8 k buffers) where the custom plan is a range on
/// `deployments_full_snapshots_ix` (60 buffers), and a fixed-format timestamp cannot inject.
pub fn scan_sql(pass: &Pass) -> String {
    match pass {
        Pass::Full => format!("{SCAN_HEAD}\n{SCAN_TAIL}"),
        Pass::Incremental { since, .. } => format!(
            "{SCAN_HEAD}\n      AND d.local_timestamp > '{}'::timestamp\n{SCAN_TAIL}",
            since.format("%Y-%m-%d %H:%M:%S%.6f")
        ),
    }
}

const MERGE: &str = "INSERT INTO marketplace.wearable_last_seen AS s (urn, last_seen, refreshed_at)
SELECT u, ts, now() FROM unnest($1::text[], $2::timestamp[]) AS t(u, ts)
ON CONFLICT (urn) DO UPDATE";

pub fn merge_sql(pass: &Pass) -> String {
    match pass {
        Pass::Full => format!("{MERGE}\n  SET last_seen = EXCLUDED.last_seen, refreshed_at = now()"),
        Pass::Incremental { .. } => format!(
            "{MERGE}\n  SET last_seen = GREATEST(s.last_seen, EXCLUDED.last_seen), refreshed_at = now()\
             \n  WHERE s.last_seen < EXCLUDED.last_seen"
        ),
    }
}

const STATE_LOAD: &str =
    "SELECT scanned_through, last_full_at FROM marketplace.wearable_last_seen_refresh WHERE id";

const STATE_SAVE: &str =
    "INSERT INTO marketplace.wearable_last_seen_refresh (id, scanned_through, last_full_at)
VALUES (true, $1, $2)
ON CONFLICT (id) DO UPDATE
  SET scanned_through = EXCLUDED.scanned_through, last_full_at = EXCLUDED.last_full_at";

const CONTENT_CLOCK: &str = "SELECT now() AT TIME ZONE 'UTC'";

#[derive(Debug)]
pub struct Outcome {
    pub pass: Pass,
    pub urns: usize,
    pub written: u64,
}

/// One pass. A full pass that finds no worn URN at all writes nothing and leaves the state alone,
/// so the next tick retries it; an incremental pass that finds nothing new still advances the
/// watermark.
pub async fn refresh(content: &PgPool, dapps: &PgPool, now: DateTime<Utc>) -> Result<Outcome> {
    let state: Option<(NaiveDateTime, DateTime<Utc>)> =
        sqlx::query_as(STATE_LOAD).fetch_optional(dapps).await?;
    let state = state.map(|(scanned_through, last_full_at)| RefreshState {
        scanned_through,
        last_full_at,
    });
    let pass = plan_pass(state.as_ref(), now);

    let scanned_through: NaiveDateTime =
        sqlx::query_scalar(CONTENT_CLOCK).fetch_one(content).await?;
    let rows: Vec<(String, NaiveDateTime)> = sqlx::query_as(sqlx::AssertSqlSafe(scan_sql(&pass)))
        .fetch_all(content)
        .await?;
    let urns = rows.len();
    if urns == 0 && matches!(pass, Pass::Full) {
        return Ok(Outcome {
            pass,
            urns,
            written: 0,
        });
    }

    let next = next_state(&pass, scanned_through, now);
    let mut tx = dapps.begin().await?;
    let mut written = 0;
    if urns > 0 {
        let (u, ts): (Vec<String>, Vec<NaiveDateTime>) = rows.into_iter().unzip();
        written = sqlx::query(sqlx::AssertSqlSafe(merge_sql(&pass)))
            .bind(&u)
            .bind(&ts)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    sqlx::query(STATE_SAVE)
        .bind(next.scanned_through)
        .bind(next.last_full_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    if written > 0 {
        sqlx::query("SELECT pg_notify('catalyrst_market_dirty', 'wearable_last_seen')")
            .execute(dapps)
            .await
            .ok();
    }
    Ok(Outcome {
        pass,
        urns,
        written,
    })
}

pub fn spawn_refresh(dapps: PgPool, content_url: String) {
    tokio::spawn(async move {
        let content = match catalyrst_db::connect_pool(
            &content_url,
            &catalyrst_db::PoolSettings {
                max_connections: 2,
                idle_timeout_secs: 600,
                ..catalyrst_db::PoolSettings::default()
            },
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "content DB unreachable: wearable_last_seen refresher off");
                return;
            }
        };
        spawn_periodic(
            "wearable-last-seen-refresh",
            REFRESH_INTERVAL,
            PeriodicCfg::default(),
            CancellationToken::new(),
            move || {
                let content = content.clone();
                let dapps = dapps.clone();
                async move {
                    let outcome = refresh(&content, &dapps, Utc::now()).await?;
                    match outcome {
                        Outcome {
                            pass: Pass::Full,
                            urns: 0,
                            ..
                        } => tracing::warn!(
                            "wearable_last_seen refresh: content DB returned no worn URNs"
                        ),
                        Outcome {
                            pass,
                            urns,
                            written,
                        } => tracing::debug!(
                            pass = match pass {
                                Pass::Full => "full",
                                Pass::Incremental { .. } => "incremental",
                            },
                            urns,
                            written,
                            "wearable_last_seen refreshed"
                        ),
                    }
                    Ok::<(), anyhow::Error>(())
                }
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
    }

    fn state(scanned_through: &str, last_full_at: &str) -> RefreshState {
        RefreshState {
            scanned_through: at(scanned_through).naive_utc(),
            last_full_at: at(last_full_at),
        }
    }

    #[test]
    fn without_a_watermark_the_pass_is_full() {
        assert_eq!(plan_pass(None, at("2026-09-17 12:00:00")), Pass::Full);
    }

    #[test]
    fn a_recent_full_pass_makes_the_next_one_incremental_from_the_margin() {
        let s = state("2026-09-17 11:50:00", "2026-09-17 01:00:00");
        assert_eq!(
            plan_pass(Some(&s), at("2026-09-17 12:00:00")),
            Pass::Incremental {
                since: at("2026-09-17 10:50:00").naive_utc(),
                last_full_at: at("2026-09-17 01:00:00"),
            }
        );
    }

    #[test]
    fn the_full_pass_comes_due_after_its_interval() {
        let s = state("2026-09-17 11:50:00", "2026-09-16 12:00:00");
        assert_eq!(plan_pass(Some(&s), at("2026-09-17 12:00:00")), Pass::Full);
        assert!(matches!(
            plan_pass(Some(&s), at("2026-09-17 11:59:59")),
            Pass::Incremental { .. }
        ));
    }

    #[test]
    fn a_full_pass_stamps_both_marks_and_an_incremental_one_only_the_watermark() {
        let clock = at("2026-09-17 12:00:03").naive_utc();
        let now = at("2026-09-17 12:00:00");
        assert_eq!(
            next_state(&Pass::Full, clock, now),
            RefreshState {
                scanned_through: clock,
                last_full_at: now
            }
        );
        let inc = Pass::Incremental {
            since: clock,
            last_full_at: at("2026-09-17 01:00:00"),
        };
        assert_eq!(
            next_state(&inc, clock, now),
            RefreshState {
                scanned_through: clock,
                last_full_at: at("2026-09-17 01:00:00")
            }
        );
    }

    #[test]
    fn the_watermark_walks_forward_across_ticks_and_the_full_pass_recurs() {
        let t0 = at("2026-09-17 00:00:00");
        let c0 = at("2026-09-17 00:00:01").naive_utc();
        let s0 = next_state(&plan_pass(None, t0), c0, t0);
        assert_eq!(s0.last_full_at, t0);

        let t1 = t0 + TimeDelta::minutes(10);
        let c1 = c0 + TimeDelta::minutes(10);
        let p1 = plan_pass(Some(&s0), t1);
        assert_eq!(
            p1,
            Pass::Incremental {
                since: c0 - SAFETY_MARGIN,
                last_full_at: t0
            }
        );
        let s1 = next_state(&p1, c1, t1);
        assert_eq!(s1.scanned_through, c1);
        assert_eq!(s1.last_full_at, t0);

        let t2 = t0 + FULL_PASS_INTERVAL;
        assert_eq!(plan_pass(Some(&s1), t2), Pass::Full);
        let s2 = next_state(&Pass::Full, c1 + FULL_PASS_INTERVAL, t2);
        assert_eq!(s2.last_full_at, t2);
    }

    #[test]
    fn the_incremental_scan_keeps_the_window_and_adds_the_watermark() {
        let inc = Pass::Incremental {
            since: at("2026-09-17 00:00:00").naive_utc(),
            last_full_at: at("2026-09-17 00:00:00"),
        };
        let full = scan_sql(&Pass::Full);
        let incremental = scan_sql(&inc);
        for sql in [&full, &incremental] {
            assert!(sql.contains("d.entity_type = 'profile'"));
            assert!(sql.contains("d.deleter_deployment IS NULL"));
            assert!(sql.contains("d.entity_timestamp > now() - interval '30 days'"));
            assert!(sql.trim_end().ends_with("GROUP BY 1"));
            assert!(sql.contains("GROUP BY w.urn\n) worn"));
        }
        assert!(!full.contains("local_timestamp"));
        assert!(
            incremental.contains("AND d.local_timestamp > '2026-09-17 00:00:00.000000'::timestamp")
        );
    }

    #[test]
    fn the_full_merge_overwrites_and_the_incremental_merge_never_lowers() {
        let inc = Pass::Incremental {
            since: at("2026-09-17 00:00:00").naive_utc(),
            last_full_at: at("2026-09-17 00:00:00"),
        };
        let full = merge_sql(&Pass::Full);
        assert!(full.contains("SET last_seen = EXCLUDED.last_seen"));
        assert!(!full.contains("GREATEST"));
        assert!(!full.contains("WHERE"));
        let incremental = merge_sql(&inc);
        assert!(incremental.contains("SET last_seen = GREATEST(s.last_seen, EXCLUDED.last_seen)"));
        assert!(incremental.contains("WHERE s.last_seen < EXCLUDED.last_seen"));
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use catalyrst_contract_gate::pg::ScratchDb;

    async fn latest_wearer(pool: &PgPool) -> i32 {
        sqlx::query_scalar(
            "SELECT d.id FROM marketplace.wearable_last_seen w JOIN deployments d
             ON d.entity_timestamp = w.last_seen WHERE w.urn LIKE 'urn:%'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn normalized_wearables_keep_latest_active_timestamp() {
        let Some(scratch) = ScratchDb::builder("CATALYRST_MARKET_TEST_PG", "worn")
            .schemas(["marketplace"])
            .build()
            .await
        else {
            return;
        };
        scratch
            .apply_sql(
                "CREATE TABLE deployments (id int, entity_type text, deleter_deployment int,
                    entity_timestamp timestamp, local_timestamp timestamp, entity_metadata json);
                 CREATE TABLE marketplace.wearable_last_seen
                    (urn text PRIMARY KEY, last_seen timestamp, refreshed_at timestamptz);
                 CREATE TABLE marketplace.wearable_last_seen_refresh (
                    id boolean PRIMARY KEY DEFAULT true CHECK (id),
                    scanned_through timestamp NOT NULL, last_full_at timestamptz NOT NULL);",
            )
            .await;
        for (id, days, kind, deleted, avatars) in [
            (
                1,
                4,
                "profile",
                None,
                serde_json::json!([
                    {"avatar":{"wearables":["URN:DCL:ETHEREUM:COLLECTIONS-V2:0xABC:7:1", "short", "short"]}},
                    {"avatar":{"wearables":["urn:dcl:ethereum:collections-v2:0xabc:7:2"]}}
                ]),
            ),
            (
                2,
                2,
                "profile",
                None,
                serde_json::json!([
                    {"avatar":{"wearables":["urn:dcl:ethereum:collections-v2:0xabc:7:3"]}}
                ]),
            ),
            (
                3,
                1,
                "profile",
                Some(9),
                serde_json::json!([
                    {"avatar":{"wearables":["urn:dcl:ethereum:collections-v2:0xabc:7:4"]}}
                ]),
            ),
            (
                4,
                31,
                "profile",
                None,
                serde_json::json!([
                    {"avatar":{"wearables":["expired"]}}
                ]),
            ),
            (
                5,
                1,
                "scene",
                None,
                serde_json::json!([
                    {"avatar":{"wearables":["scene"]}}
                ]),
            ),
            (
                6,
                1,
                "profile",
                None,
                serde_json::json!([{"avatar":{}},{"avatar":{"wearables":[]}}]),
            ),
        ] {
            sqlx::query(
                "INSERT INTO deployments VALUES
                 ($1, $2, $3, now() - $4 * interval '1 day', now() - $4 * interval '1 day', $5::json)",
            )
            .bind(id)
            .bind(kind)
            .bind(deleted)
            .bind(days)
            .bind(serde_json::json!({"v":{"avatars":avatars}}).to_string())
            .execute(&scratch.pool)
            .await
            .unwrap();
        }

        let t0 = Utc::now();
        let first = refresh(&scratch.pool, &scratch.pool, t0).await.unwrap();
        assert_eq!(first.pass, Pass::Full);
        assert_eq!(first.urns, 2);
        let rows: Vec<(String, i32)> = sqlx::query_as(
            "SELECT w.urn, d.id FROM marketplace.wearable_last_seen w
             JOIN deployments d ON d.entity_timestamp = w.last_seen ORDER BY w.urn",
        )
        .fetch_all(&scratch.pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                ("short".into(), 1),
                ("urn:dcl:ethereum:collections-v2:0xabc:7".into(), 2)
            ]
        );

        sqlx::query("UPDATE deployments SET deleter_deployment = 9 WHERE id = 2")
            .execute(&scratch.pool)
            .await
            .unwrap();
        let incremental = refresh(&scratch.pool, &scratch.pool, t0 + TimeDelta::minutes(10))
            .await
            .unwrap();
        assert!(matches!(incremental.pass, Pass::Incremental { .. }));
        assert_eq!(
            latest_wearer(&scratch.pool).await,
            2,
            "an incremental pass never lowers a value"
        );

        let full = refresh(&scratch.pool, &scratch.pool, t0 + FULL_PASS_INTERVAL)
            .await
            .unwrap();
        assert_eq!(full.pass, Pass::Full);
        assert_eq!(
            latest_wearer(&scratch.pool).await,
            1,
            "the full pass moves last_seen back to the newest active wearer"
        );
        scratch.drop().await;
    }
}
