pub mod derive;
pub mod mirror;
pub mod sync;
pub mod worlds_mirror;

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Parse before binding, never through `::timestamptz`: a mirrored row carries
/// a third-party string, and a cast error there fails the whole page instead of
/// dropping the one unusable field.
pub(crate) fn parse_mirror_timestamp(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let raw = raw?;
    match DateTime::parse_from_rfc3339(raw) {
        Ok(d) => Some(d.with_timezone(&Utc)),
        Err(_) => {
            tracing::debug!(value = raw, "mirror: unparseable deployed_at dropped");
            None
        }
    }
}

const PLACE_BASE: &str = include_str!("../../migrations/0000_place.sql");
const PLACE_INDEXED: &str = include_str!("../../migrations/0002_place_indexed.sql");
const PLACE_WORLD_NAME: &str = include_str!("../../migrations/0003_place_world_name.sql");
const PLACE_PLAIN_TEXT: &str = include_str!("../../migrations/0004_place_plain_text.sql");
const ROAD_POSITIONS: &str = include_str!("../../migrations/0005_road_positions.sql");

pub async fn ensure_road_positions(pool: &PgPool) -> Result<()> {
    let existing: Option<String> = sqlx::query_scalar("SELECT to_regclass('road_positions')::text")
        .fetch_one(pool)
        .await?;
    if existing.is_some() {
        return Ok(());
    }
    sqlx::raw_sql(ROAD_POSITIONS).execute(pool).await?;
    Ok(())
}

pub async fn ensure_schema(pool: &PgPool) -> Result<()> {
    let existing: Option<String> = sqlx::query_scalar("SELECT to_regclass('place_indexed')::text")
        .fetch_one(pool)
        .await?;
    if existing.is_some() {
        return Ok(());
    }
    for statement in [
        PLACE_BASE,
        PLACE_INDEXED,
        PLACE_WORLD_NAME,
        PLACE_PLAIN_TEXT,
    ] {
        sqlx::raw_sql(statement).execute(pool).await?;
    }
    Ok(())
}
