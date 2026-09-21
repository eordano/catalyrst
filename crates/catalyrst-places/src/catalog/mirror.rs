use std::collections::HashMap;
use std::time::Duration;

use anyhow::{bail, Result};
use catalyrst_commons::http::{http_client, HttpClientCfg};
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::catalog::parse_mirror_timestamp;

const PAGE: i64 = 100;
const INTERVAL: Duration = Duration::from_secs(3600);
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; catalyrst-places-mirror/1; +https://decentraland.org)";

const UPSERT: &str = r#"
    INSERT INTO place
        (id, base_position, title, description, creator_address, content_rating,
         categories, likes, dislikes, favorites, deployed_at, disabled, highlighted,
         raw, fetched_at)
    SELECT u.id, u.base_position, u.title, u.description, u.creator_address, u.content_rating,
           ARRAY(SELECT jsonb_array_elements_text(u.categories)), u.likes, u.dislikes, u.favorites,
           u.deployed_at, u.disabled, u.highlighted, u.raw, now()
    FROM unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[],
                $7::jsonb[], $8::int4[], $9::int4[], $10::int4[], $11::timestamptz[],
                $12::boolean[], $13::boolean[], $14::jsonb[])
         AS u(id, base_position, title, description, creator_address, content_rating,
              categories, likes, dislikes, favorites, deployed_at, disabled, highlighted, raw)
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

const RETIRE_DERIVED: &str = "DELETE FROM place WHERE raw->>'source' = 'content'";

pub fn spawn(pool: PgPool, upstream_url: String) {
    let client = http_client(
        "places-mirror",
        &HttpClientCfg::default()
            .following_redirects(10)
            .with_user_agent(USER_AGENT),
    );
    spawn_periodic(
        "place-catalog-mirror",
        INTERVAL,
        PeriodicCfg::default(),
        CancellationToken::new(),
        move || {
            let pool = pool.clone();
            let client = client.clone();
            let upstream_url = upstream_url.clone();
            async move {
                match run_once(&pool, &client, &upstream_url).await {
                    Ok((mirrored, retired)) => {
                        tracing::info!(mirrored, retired, "place catalog mirrored from upstream")
                    }
                    Err(e) => tracing::warn!(error = %e, "place catalog mirror cycle failed"),
                }
                Ok::<(), anyhow::Error>(())
            }
        },
    );
}

pub async fn run_once(
    pool: &PgPool,
    client: &reqwest::Client,
    upstream: &str,
) -> Result<(usize, u64)> {
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
        upsert_page(pool, &data).await?;
        mirrored += data.len();
        offset += data.len() as i64;
        if offset >= total || (data.len() as i64) < PAGE {
            break;
        }
    }
    let retired = if mirrored > 0 {
        sqlx::query(RETIRE_DERIVED)
            .execute(pool)
            .await?
            .rows_affected()
    } else {
        0
    };
    Ok((mirrored, retired))
}

fn first_str<'a>(place: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| place.get(*k).and_then(Value::as_str))
}

fn int(place: &Value, key: &str) -> i32 {
    place.get(key).and_then(Value::as_i64).unwrap_or(0) as i32
}

struct MirrorRow<'a> {
    id: &'a str,
    base_position: &'a str,
    title: &'a str,
    description: &'a str,
    creator_address: Option<String>,
    content_rating: Option<&'a str>,
    categories: Value,
    likes: i32,
    dislikes: i32,
    favorites: i32,
    deployed_at: Option<DateTime<Utc>>,
    disabled: bool,
    highlighted: bool,
    raw: &'a Value,
}

fn extract(place: &Value) -> Option<MirrorRow<'_>> {
    let id = match place.get("id").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s,
        _ => return None,
    };
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
    Some(MirrorRow {
        id,
        base_position: place
            .get("base_position")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("0,0"),
        title: first_str(place, &["title", "name"]).unwrap_or(""),
        description: place
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
        creator_address: first_str(place, &["owner", "creator_address"])
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty()),
        content_rating: place.get("content_rating").and_then(Value::as_str),
        categories: Value::from(categories),
        likes: int(place, "likes"),
        dislikes: int(place, "dislikes"),
        favorites: int(place, "favorites"),
        deployed_at: parse_mirror_timestamp(place.get("deployed_at").and_then(Value::as_str)),
        disabled: place
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        highlighted: place
            .get("highlighted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        raw: place,
    })
}

/// One multi-row upsert per upstream page; a later duplicate id wins as it did row by row.
async fn upsert_page(pool: &PgPool, data: &[Value]) -> Result<()> {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    let mut rows: Vec<MirrorRow> = Vec::with_capacity(data.len());
    for row in data.iter().filter_map(extract) {
        match by_id.get(row.id) {
            Some(&i) => rows[i] = row,
            None => {
                by_id.insert(row.id, rows.len());
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        return Ok(());
    }
    sqlx::query(UPSERT)
        .bind(rows.iter().map(|r| r.id).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.base_position).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.title).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.description).collect::<Vec<_>>())
        .bind(
            rows.iter()
                .map(|r| r.creator_address.clone())
                .collect::<Vec<_>>(),
        )
        .bind(rows.iter().map(|r| r.content_rating).collect::<Vec<_>>())
        .bind(
            rows.iter()
                .map(|r| r.categories.clone())
                .collect::<Vec<_>>(),
        )
        .bind(rows.iter().map(|r| r.likes).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.dislikes).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.favorites).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.deployed_at).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.disabled).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.highlighted).collect::<Vec<_>>())
        .bind(rows.iter().map(|r| r.raw.clone()).collect::<Vec<_>>())
        .execute(pool)
        .await?;
    Ok(())
}
