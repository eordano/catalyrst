use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;
use sqlx::PgPool;

const PAGE: i64 = 100;
const INTERVAL: Duration = Duration::from_secs(3600);
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; catalyrst-places-mirror/1; +https://decentraland.org)";

const UPSERT: &str = r#"
    INSERT INTO place
        (id, base_position, title, description, creator_address, content_rating,
         categories, likes, dislikes, favorites, deployed_at, disabled, highlighted,
         raw, fetched_at)
    VALUES
        ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::timestamptz, $12, $13, $14, now())
    ON CONFLICT (id) DO UPDATE SET
        base_position   = EXCLUDED.base_position,
        title           = EXCLUDED.title,
        description     = EXCLUDED.description,
        creator_address = EXCLUDED.creator_address,
        content_rating  = EXCLUDED.content_rating,
        categories      = EXCLUDED.categories,
        likes           = EXCLUDED.likes,
        dislikes        = EXCLUDED.dislikes,
        favorites       = EXCLUDED.favorites,
        deployed_at     = EXCLUDED.deployed_at,
        disabled        = EXCLUDED.disabled,
        highlighted     = EXCLUDED.highlighted,
        raw             = EXCLUDED.raw,
        fetched_at      = now()
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
                tracing::warn!(error = %e, "place mirror: http client build failed; disabled");
                return;
            }
        };
        loop {
            match run_once(&pool, &client, &upstream_url).await {
                Ok(n) => tracing::info!(mirrored = n, "place catalog mirrored from upstream"),
                Err(e) => tracing::warn!(error = %e, "place catalog mirror cycle failed"),
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
        let url = format!("{base}/api/places?limit={PAGE}&offset={offset}");
        let body: Value = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            bail!("places upstream returned ok=false at offset={offset}");
        }
        let total = body.get("total").and_then(Value::as_i64).unwrap_or(0);
        let data = match body.get("data").and_then(Value::as_array) {
            Some(a) if !a.is_empty() => a.clone(),
            _ => break,
        };
        for place in &data {
            upsert(pool, place).await?;
            mirrored += 1;
        }
        offset += data.len() as i64;
        if offset >= total || (data.len() as i64) < PAGE {
            break;
        }
    }
    Ok(mirrored)
}

fn first_str<'a>(place: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| place.get(*k).and_then(Value::as_str))
}

fn int(place: &Value, key: &str) -> i32 {
    place.get(key).and_then(Value::as_i64).unwrap_or(0) as i32
}

async fn upsert(pool: &PgPool, place: &Value) -> Result<()> {
    let id = match place.get("id").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(()),
    };
    let base_position = place
        .get("base_position")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("0,0");
    let title = first_str(place, &["title", "name"]).unwrap_or("");
    let creator_address = first_str(place, &["owner", "creator_address"])
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty());
    let categories: Vec<String> = place
        .get("categories")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    sqlx::query(UPSERT)
        .bind(id)
        .bind(base_position)
        .bind(title)
        .bind(
            place
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .bind(creator_address)
        .bind(place.get("content_rating").and_then(Value::as_str))
        .bind(&categories)
        .bind(int(place, "likes"))
        .bind(int(place, "dislikes"))
        .bind(int(place, "favorites"))
        .bind(place.get("deployed_at").and_then(Value::as_str))
        .bind(
            place
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(
            place
                .get("highlighted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
        .bind(place)
        .execute(pool)
        .await?;
    Ok(())
}
