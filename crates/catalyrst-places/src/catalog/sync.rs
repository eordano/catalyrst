use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::catalog::derive::{derive, DerivedPlace};

const PAGE: i64 = 1000;
const INTERVAL: Duration = Duration::from_secs(3600);

const SELECT_SCENES: &str = r#"
    SELECT
        d.deployer_address,
        d.entity_pointers,
        (d.entity_timestamp AT TIME ZONE 'UTC') AS deployed_at,
        (d.entity_metadata::jsonb) AS meta,
        (SELECT cf.content_hash FROM content_files cf
          WHERE cf.deployment = d.id
            AND cf.key = (d.entity_metadata::jsonb)->'v'->'display'->>'navmapThumbnail'
          LIMIT 1) AS thumbnail_hash
    FROM deployments d
    WHERE d.entity_type = 'scene' AND d.deleter_deployment IS NULL
    ORDER BY d.id
    LIMIT $1 OFFSET $2
"#;

const UPSERT: &str = r#"
    INSERT INTO place
        (id, base_position, title, description, creator_address, content_rating,
         categories, likes, dislikes, favorites, deployed_at, disabled, highlighted,
         raw, fetched_at)
    VALUES ($1, $2, $3, $4, $5, $6, '{}', 0, 0, 0, $7, false, false, $8, now())
    ON CONFLICT (id) DO UPDATE SET
        base_position   = EXCLUDED.base_position,
        title           = EXCLUDED.title,
        description     = EXCLUDED.description,
        creator_address = EXCLUDED.creator_address,
        deployed_at     = EXCLUDED.deployed_at,
        raw = EXCLUDED.raw || jsonb_strip_nulls(jsonb_build_object(
            'ranking',           place.raw->'ranking',
            'highlighted_image', place.raw->'highlighted_image',
            'like_score',        place.raw->'like_score',
            'like_rate',         place.raw->'like_rate'
        )),
        fetched_at = now()
"#;

const PRUNE: &str = r#"
    DELETE FROM place WHERE raw->>'source' = 'content' AND fetched_at < $1
"#;

pub fn spawn(places: PgPool, content: PgPool, content_public_url: String) {
    tokio::spawn(async move {
        loop {
            match run_once(&places, &content, &content_public_url).await {
                Ok((derived, pruned)) => tracing::info!(
                    derived,
                    pruned,
                    "place catalog synced from content deployments"
                ),
                Err(e) => tracing::warn!(error = %e, "place catalog sync failed"),
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

async fn run_once(
    places: &PgPool,
    content: &PgPool,
    content_public_url: &str,
) -> Result<(usize, u64)> {
    let started: DateTime<Utc> = sqlx::query_scalar("SELECT now()").fetch_one(places).await?;
    let mut offset = 0i64;
    let mut derived = 0usize;
    loop {
        let rows = sqlx::query(SELECT_SCENES)
            .bind(PAGE)
            .bind(offset)
            .fetch_all(content)
            .await?;
        let fetched = rows.len() as i64;
        for row in &rows {
            let deployer: String = row.try_get("deployer_address").unwrap_or_default();
            let pointers: Vec<String> = row.try_get("entity_pointers").unwrap_or_default();
            let deployed_at: Option<DateTime<Utc>> = row.try_get("deployed_at").unwrap_or(None);
            let meta: Value = row.try_get("meta").unwrap_or(Value::Null);
            let thumb: Option<String> = row.try_get("thumbnail_hash").unwrap_or(None);
            if let Some(p) = derive(
                &deployer,
                &pointers,
                deployed_at,
                &meta,
                thumb.as_deref(),
                content_public_url,
            ) {
                upsert(places, &p).await?;
                derived += 1;
            }
        }
        if fetched < PAGE {
            break;
        }
        offset += fetched;
    }
    let pruned = sqlx::query(PRUNE)
        .bind(started)
        .execute(places)
        .await?
        .rows_affected();
    Ok((derived, pruned))
}

async fn upsert(places: &PgPool, p: &DerivedPlace) -> Result<()> {
    sqlx::query(UPSERT)
        .bind(&p.id)
        .bind(&p.base_position)
        .bind(&p.title)
        .bind(&p.description)
        .bind(&p.creator_address)
        .bind(&p.content_rating)
        .bind(p.deployed_at)
        .bind(&p.raw)
        .execute(places)
        .await?;
    Ok(())
}
