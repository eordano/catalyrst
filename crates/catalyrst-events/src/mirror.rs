use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;
use sqlx::PgPool;

const PAGE: i64 = 100;
const INTERVAL: Duration = Duration::from_secs(3600);
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; catalyrst-events-mirror/1; +https://decentraland.org)";

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
"#;

pub fn spawn(pool: PgPool, upstream_url: String) {
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
            match run_once(&pool, &client, &upstream_url).await {
                Ok(n) => tracing::info!(mirrored = n, "event catalog mirrored from upstream"),
                Err(e) => tracing::warn!(error = %e, "event catalog mirror cycle failed"),
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

async fn run_once(pool: &PgPool, client: &reqwest::Client, upstream: &str) -> Result<usize> {
    let base = upstream.trim_end_matches('/');
    let mut offset = 0i64;
    let mut mirrored = 0usize;
    loop {
        let url = format!("{base}/api/events?limit={PAGE}&offset={offset}");
        let body: Value = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            bail!("events upstream returned ok=false at offset={offset}");
        }
        let data = match body.get("data").and_then(Value::as_array) {
            Some(a) if !a.is_empty() => a.clone(),
            _ => break,
        };
        let count = data.len();
        for event in &data {
            upsert(pool, event).await?;
            mirrored += 1;
        }
        if (count as i64) < PAGE {
            break;
        }
        offset += PAGE;
    }
    Ok(mirrored)
}

fn coord(event: &Value, idx: usize) -> Option<i32> {
    event
        .get("coordinates")
        .and_then(Value::as_array)
        .and_then(|a| a.get(idx))
        .and_then(Value::as_i64)
        .map(|v| v as i32)
}

async fn upsert(pool: &PgPool, event: &Value) -> Result<()> {
    let id = match event.get("id").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(()),
    };
    sqlx::query(UPSERT)
        .bind(id)
        .bind(event.get("name").and_then(Value::as_str).unwrap_or(""))
        .bind(event.get("start_at").and_then(Value::as_str))
        .bind(event.get("finish_at").and_then(Value::as_str))
        .bind(event.get("next_start_at").and_then(Value::as_str))
        .bind(event.get("next_finish_at").and_then(Value::as_str))
        .bind(event.get("duration").and_then(Value::as_i64))
        .bind(
            event
                .get("recurrent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(
            event
                .get("highlighted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(
            event
                .get("trending")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(
            event
                .get("approved")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(event.get("attending").and_then(Value::as_bool))
        .bind(event.get("community_id").and_then(Value::as_str))
        .bind(event.get("user").and_then(Value::as_str))
        .bind(coord(event, 0))
        .bind(coord(event, 1))
        .bind(
            event
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .bind(event)
        .execute(pool)
        .await?;
    Ok(())
}
