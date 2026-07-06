use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProposalVotesPayload {
    pub choices: Vec<String>,
    pub scores: Vec<f64>,
    pub scores_total: f64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub votes_count: i64,
    pub votes: Vec<ProposalVoteItem>,
    pub series: Option<VpSeriesPayload>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ProposalVoteItem {
    pub voter: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub choice: i64,
    pub vp: f64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_ts: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct VpSeriesPayload {
    pub ticks: Vec<String>,
    pub choice1: Vec<f64>,
    pub choice2: Vec<f64>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct CommentsPayload {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
    pub comments: Vec<CommentItem>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct CommentItem {
    pub username: String,
    pub created_at: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct EngagementPayload {
    pub voters: Vec<TopVoterItem>,
    pub weekly: Vec<WeeklyBucket>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct TopVoterItem {
    pub address: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub votes: i64,
    pub vp: f64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct WeeklyBucket {
    pub week_start: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub votes: i64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ActivityPayload {
    pub items: Vec<ActivityFeedItem>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct ActivityFeedItem {
    pub kind: String,
    pub address: Option<String>,
    pub title: Option<String>,
    pub proposal_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ts: i64,
}

pub struct Archives {
    pub snapshot: Option<PgPool>,
    pub discourse: Option<PgPool>,
}

impl Archives {
    pub async fn from_env() -> Self {
        Self {
            snapshot: optional_pool("SNAPSHOT_DATABASE_URL", "snapshot").await,
            discourse: optional_pool("DISCOURSE_DATABASE_URL", "discourse").await,
        }
    }
}

async fn optional_pool(key: &str, label: &str) -> Option<PgPool> {
    let url = std::env::var(key).ok().filter(|s| !s.is_empty())?;
    match connect(&url).await {
        Ok(pool) => {
            tracing::info!(archive = label, "governance archive pool connected");
            Some(pool)
        }
        Err(err) => {
            tracing::warn!(archive = label, %err, "governance archive pool unavailable; serving honest empties");
            None
        }
    }
}

async fn connect(url: &str) -> Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)
        .context("invalid archive database url")?
        .options([("statement_timeout", "15000")]);
    PgPoolOptions::new()
        .max_connections(2)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("archive pool connect failed")
}

const MAX_VOTES_PER_PROPOSAL: i64 = 20_000;
const RATIONALE_VOTES_CAP: usize = 400;
const SERIES_TICKS: usize = 24;

pub fn empty_votes_payload() -> ProposalVotesPayload {
    ProposalVotesPayload {
        choices: Vec::new(),
        scores: Vec::new(),
        scores_total: 0.0,
        votes_count: 0,
        votes: Vec::new(),
        series: None,
    }
}

pub async fn proposal_votes(pool: &PgPool, snapshot_id: &str) -> Result<ProposalVotesPayload> {
    let head = sqlx::query(
        "SELECT choices, scores, COALESCE(scores_total, 0) AS scores_total,
                COALESCE(votes_count, 0) AS votes_count
         FROM proposals WHERE id = $1",
    )
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .context("snapshot proposal head")?;

    let Some(head) = head else {
        return Ok(empty_votes_payload());
    };
    let choices: Vec<String> = head
        .try_get::<Option<serde_json::Value>, _>("choices")?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let scores: Vec<f64> = head
        .try_get::<Option<serde_json::Value>, _>("scores")?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let scores_total: f64 = head.try_get("scores_total")?;
    let votes_count: i64 = head
        .try_get::<i32, _>("votes_count")
        .map(i64::from)
        .or_else(|_| head.try_get::<i64, _>("votes_count"))?;

    let rows = sqlx::query(
        "SELECT voter, choice, COALESCE(vp, 0) AS vp, created_ts, reason
         FROM votes WHERE proposal_id = $1
         ORDER BY created_ts ASC LIMIT $2",
    )
    .bind(snapshot_id)
    .bind(MAX_VOTES_PER_PROPOSAL)
    .fetch_all(pool)
    .await
    .context("snapshot votes")?;

    let mut all: Vec<ProposalVoteItem> = rows
        .iter()
        .map(|r| {
            let choice = r
                .try_get::<Option<serde_json::Value>, _>("choice")
                .ok()
                .flatten()
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            ProposalVoteItem {
                voter: r.try_get::<String, _>("voter").unwrap_or_default(),
                choice,
                vp: r.try_get::<f64, _>("vp").unwrap_or(0.0),
                created_ts: r.try_get::<i64, _>("created_ts").unwrap_or(0),
                reason: r.try_get::<Option<String>, _>("reason").ok().flatten(),
            }
        })
        .collect();

    let series = if choices.len() == 2 && all.len() > 1 {
        Some(build_series(&all))
    } else {
        None
    };

    all.sort_by(|a, b| b.vp.partial_cmp(&a.vp).unwrap_or(std::cmp::Ordering::Equal));
    all.truncate(RATIONALE_VOTES_CAP);

    Ok(ProposalVotesPayload {
        choices,
        scores,
        scores_total,
        votes_count,
        votes: all,
        series,
    })
}

fn build_series(chronological: &[ProposalVoteItem]) -> VpSeriesPayload {
    let first_ts = chronological.first().map(|v| v.created_ts).unwrap_or(0);
    let last_ts = chronological.last().map(|v| v.created_ts).unwrap_or(0);
    let span = (last_ts - first_ts).max(1);
    let ticks_n = SERIES_TICKS.min(chronological.len()).max(2);

    let mut choice1 = vec![0.0f64; ticks_n];
    let mut choice2 = vec![0.0f64; ticks_n];
    for v in chronological {
        let idx = (((v.created_ts - first_ts) as f64 / span as f64) * (ticks_n as f64 - 1.0))
            .round() as usize;
        let idx = idx.min(ticks_n - 1);
        match v.choice {
            1 => choice1[idx] += v.vp,
            2 => choice2[idx] += v.vp,
            _ => {}
        }
    }
    for i in 1..ticks_n {
        choice1[i] += choice1[i - 1];
        choice2[i] += choice2[i - 1];
    }
    let ticks = (0..ticks_n)
        .map(|i| {
            let ts = first_ts + (span * i as i64) / (ticks_n as i64 - 1).max(1);
            format_day(ts)
        })
        .collect();
    VpSeriesPayload {
        ticks,
        choice1,
        choice2,
    }
}

fn format_day(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub async fn comments_by_topic(
    pool: &PgPool,
    topic_id: i64,
    limit: i64,
) -> Result<CommentsPayload> {
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM posts
         WHERE topic_id = $1 AND post_number > 1
           AND hidden = false AND deleted_at IS NULL",
    )
    .bind(topic_id)
    .fetch_one(pool)
    .await
    .context("discourse comment count")?;

    let rows = sqlx::query(
        "SELECT username, created_at, raw FROM posts
         WHERE topic_id = $1 AND post_number > 1
           AND hidden = false AND deleted_at IS NULL
         ORDER BY created_at DESC LIMIT $2",
    )
    .bind(topic_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("discourse comments")?;

    let comments = rows
        .iter()
        .map(|r| CommentItem {
            username: r.try_get::<String, _>("username").unwrap_or_default(),
            created_at: r
                .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
            text: r.try_get::<String, _>("raw").unwrap_or_default(),
        })
        .collect();

    Ok(CommentsPayload { total, comments })
}

pub async fn engagement(pool: &PgPool, days: i64, limit: i64) -> Result<EngagementPayload> {
    let cutoff = now_epoch() - days.max(1) * 86_400;

    let voters = sqlx::query(
        "SELECT lower(voter) AS address, count(*) AS votes, COALESCE(sum(vp), 0) AS vp
         FROM votes WHERE created_ts >= $1
         GROUP BY 1 ORDER BY votes DESC, vp DESC LIMIT $2",
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("top voters")?
    .iter()
    .map(|r| TopVoterItem {
        address: r.try_get::<String, _>("address").unwrap_or_default(),
        votes: r.try_get::<i64, _>("votes").unwrap_or(0),
        vp: r.try_get::<f64, _>("vp").unwrap_or(0.0),
    })
    .collect();

    let weekly = sqlx::query(
        "SELECT to_char(date_trunc('week', to_timestamp(created_ts)), 'YYYY-MM-DD') AS week_start,
                count(*) AS votes
         FROM votes WHERE created_ts >= $1
         GROUP BY 1 ORDER BY 1",
    )
    .bind(now_epoch() - 8 * 7 * 86_400)
    .fetch_all(pool)
    .await
    .context("weekly votes")?
    .iter()
    .map(|r| WeeklyBucket {
        week_start: r.try_get::<String, _>("week_start").unwrap_or_default(),
        votes: r.try_get::<i64, _>("votes").unwrap_or(0),
    })
    .collect();

    Ok(EngagementPayload { voters, weekly })
}

pub async fn recent_votes(pool: &PgPool, limit: i64) -> Result<Vec<(String, String, f64, i64)>> {
    let rows = sqlx::query(
        "SELECT voter, proposal_id, COALESCE(vp, 0) AS vp, created_ts
         FROM votes ORDER BY created_ts DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("recent votes")?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get::<String, _>("voter").unwrap_or_default(),
                r.try_get::<String, _>("proposal_id").unwrap_or_default(),
                r.try_get::<f64, _>("vp").unwrap_or(0.0),
                r.try_get::<i64, _>("created_ts").unwrap_or(0),
            )
        })
        .collect())
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote(choice: i64, vp: f64, ts: i64) -> ProposalVoteItem {
        ProposalVoteItem {
            voter: "0x0".into(),
            choice,
            vp,
            created_ts: ts,
            reason: None,
        }
    }

    #[test]
    fn series_is_cumulative_and_bucketed() {
        let votes: Vec<_> = (0..10)
            .map(|i| vote(if i % 2 == 0 { 1 } else { 2 }, 10.0, 1000 + i * 100))
            .collect();
        let s = build_series(&votes);
        assert_eq!(s.ticks.len(), s.choice1.len());
        assert_eq!(s.choice1.len(), s.choice2.len());
        assert_eq!(*s.choice1.last().unwrap(), 50.0);
        assert_eq!(*s.choice2.last().unwrap(), 50.0);
        assert!(s.choice1.windows(2).all(|w| w[0] <= w[1]));
        assert!(s.choice2.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn series_ignores_weighted_choices() {
        let votes = vec![vote(0, 99.0, 0), vote(1, 1.0, 10)];
        let s = build_series(&votes);
        assert_eq!(*s.choice1.last().unwrap(), 1.0);
        assert_eq!(*s.choice2.last().unwrap(), 0.0);
    }

    #[test]
    fn civil_date_conversion() {
        assert_eq!(format_day(0), "1970-01-01");
        assert_eq!(format_day(86_400), "1970-01-02");
        assert_eq!(format_day(1_767_225_600), "2026-01-01");
    }
}
