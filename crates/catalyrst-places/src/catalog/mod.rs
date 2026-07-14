pub mod derive;
pub mod mirror;
pub mod sync;
pub mod worlds_mirror;

use anyhow::Result;
use sqlx::PgPool;

const PLACE_BASE: &str = include_str!("../../migrations/0000_place.sql");
const PLACE_INDEXED: &str = include_str!("../../migrations/0002_place_indexed.sql");
const PLACE_WORLD_NAME: &str = include_str!("../../migrations/0003_place_world_name.sql");
const PLACE_PLAIN_TEXT: &str = include_str!("../../migrations/0004_place_plain_text.sql");

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
