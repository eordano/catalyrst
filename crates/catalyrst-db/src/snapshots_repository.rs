use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct TimeRange {
    pub init_timestamp: f64,
    pub end_timestamp: f64,
}

impl TimeRange {
    pub fn new(init_timestamp: f64, end_timestamp: f64) -> Self {
        Self {
            init_timestamp,
            end_timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSyncDeployment {
    #[sqlx(rename = "entityId")]
    pub entity_id: String,
    #[sqlx(rename = "entityType")]
    pub entity_type: String,
    pub pointers: Vec<String>,
    #[sqlx(rename = "authChain")]
    pub auth_chain: serde_json::Value,
    // Integer ms — the canonical Decentraland snapshot format. Emitting a float
    // here (`...515.0`) makes every line unparseable by clients that deserialize
    // entityTimestamp as an integer, silently breaking snapshot bootstrap (the
    // whole sync then falls back to the slow per-entity pointer-changes path).
    #[sqlx(rename = "entityTimestamp")]
    pub entity_timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub hash: Option<String>,
    pub time_range: TimeRange,
    pub replaced_snapshot_hashes: Vec<String>,
    pub number_of_entities: i32,
    pub generation_timestamp: f64,
}

pub async fn stream_active_deployments_in_time_range(
    pool: &PgPool,
    time_range: TimeRange,
) -> Result<Vec<SnapshotSyncDeployment>, sqlx::Error> {
    sqlx::query_as::<_, SnapshotSyncDeployment>(
        r#"
        SELECT
            entity_id AS "entityId",
            entity_type AS "entityType",
            entity_pointers AS pointers,
            auth_chain AS "authChain",
            (date_part('epoch', entity_timestamp) * 1000)::bigint AS "entityTimestamp"
        FROM deployments
        WHERE deleter_deployment IS NULL
          AND entity_timestamp BETWEEN to_timestamp($1 / 1000.0) AND to_timestamp($2 / 1000.0)
        ORDER BY entity_timestamp
        "#,
    )
    .bind(time_range.init_timestamp)
    .bind(time_range.end_timestamp)
    .fetch_all(pool)
    .await
}

pub async fn find_snapshots_strictly_contained_in_time_range(
    pool: &PgPool,
    time_range: TimeRange,
) -> Result<Vec<SnapshotMetadata>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        hash: Option<String>,
        #[sqlx(rename = "initTimestamp")]
        init_timestamp: f64,
        #[sqlx(rename = "endTimestamp")]
        end_timestamp: f64,
        #[sqlx(rename = "replacedSnapshotHashes")]
        replaced_snapshot_hashes: Vec<String>,
        #[sqlx(rename = "numberOfEntities")]
        number_of_entities: i32,
        #[sqlx(rename = "generationTimestamp")]
        generation_timestamp: f64,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            hash,
            date_part('epoch', init_timestamp) * 1000 AS "initTimestamp",
            date_part('epoch', end_timestamp) * 1000 AS "endTimestamp",
            replaced_hashes AS "replacedSnapshotHashes",
            number_of_entities AS "numberOfEntities",
            date_part('epoch', generation_time) * 1000 AS "generationTimestamp"
        FROM snapshots s
        WHERE init_timestamp >= to_timestamp($1 / 1000.0)
          AND end_timestamp <= to_timestamp($2 / 1000.0)
        "#,
    )
    .bind(time_range.init_timestamp)
    .bind(time_range.end_timestamp)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SnapshotMetadata {
            hash: r.hash,
            time_range: TimeRange {
                init_timestamp: r.init_timestamp,
                end_timestamp: r.end_timestamp,
            },
            replaced_snapshot_hashes: r.replaced_snapshot_hashes,
            number_of_entities: r.number_of_entities,
            generation_timestamp: r.generation_timestamp,
        })
        .collect())
}

pub async fn save_snapshot(pool: &PgPool, snap: &SnapshotMetadata) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO snapshots
            (hash, init_timestamp, end_timestamp, replaced_hashes, number_of_entities, generation_time)
        VALUES
            ($1, to_timestamp($2 / 1000.0), to_timestamp($3 / 1000.0), $4, $5, to_timestamp($6 / 1000.0))
        RETURNING hash
        "#,
    )
    .bind(&snap.hash)
    .bind(snap.time_range.init_timestamp)
    .bind(snap.time_range.end_timestamp)
    .bind(&snap.replaced_snapshot_hashes)
    .bind(snap.number_of_entities)
    .bind(snap.generation_timestamp)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn is_own_snapshot(pool: &PgPool, snapshot_hash: &str) -> Result<bool, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT hash FROM snapshots WHERE hash = $1")
            .bind(snapshot_hash)
            .fetch_optional(pool)
            .await?;

    Ok(row.is_some())
}

pub async fn get_snapshot_hashes_not_in_time_range(
    pool: &PgPool,
    snapshot_hashes: &[String],
    time_range: TimeRange,
) -> Result<HashSet<String>, sqlx::Error> {
    if snapshot_hashes.is_empty() {
        return Ok(HashSet::new());
    }

    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        r#"
        SELECT hash
        FROM snapshots
        WHERE init_timestamp <= end_timestamp
          AND (init_timestamp >= to_timestamp($1 / 1000.0)
               OR end_timestamp <= to_timestamp($2 / 1000.0))
          AND hash = ANY($3)
        "#,
    )
    .bind(time_range.end_timestamp)
    .bind(time_range.init_timestamp)
    .bind(snapshot_hashes)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().filter_map(|r| r.0).collect())
}

pub async fn delete_snapshots_in_time_range(
    pool: &PgPool,
    snapshot_hashes: &[String],
    time_range: TimeRange,
) -> Result<(), sqlx::Error> {
    if snapshot_hashes.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        DELETE FROM snapshots
        WHERE init_timestamp >= to_timestamp($1 / 1000.0)
          AND end_timestamp <= to_timestamp($2 / 1000.0)
          AND hash = ANY($3)
        "#,
    )
    .bind(time_range.init_timestamp)
    .bind(time_range.end_timestamp)
    .bind(snapshot_hashes)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn snapshot_is_outdated(
    pool: &PgPool,
    snap: &SnapshotMetadata,
) -> Result<bool, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as(
        r#"
        SELECT 1
        FROM deployments
        WHERE deleter_deployment IS NULL
          AND entity_timestamp BETWEEN to_timestamp($1 / 1000.0) AND to_timestamp($2 / 1000.0)
          AND local_timestamp > to_timestamp($3 / 1000.0)
        "#,
    )
    .bind(snap.time_range.init_timestamp)
    .bind(snap.time_range.end_timestamp)
    .bind(snap.generation_timestamp)
    .fetch_optional(pool)
    .await?;

    Ok(row.is_some())
}

pub async fn get_number_of_active_entities_in_time_range(
    pool: &PgPool,
    time_range: TimeRange,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) AS count
        FROM deployments
        WHERE deleter_deployment IS NULL
          AND entity_timestamp BETWEEN to_timestamp($1 / 1000.0) AND to_timestamp($2 / 1000.0)
        "#,
    )
    .bind(time_range.init_timestamp)
    .bind(time_range.end_timestamp)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn get_all_snapshots(pool: &PgPool) -> Result<Vec<SnapshotMetadata>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        hash: Option<String>,
        init_ts_ms: f64,
        end_ts_ms: f64,
        replaced_hashes: Vec<String>,
        number_of_entities: i32,
        gen_ts_ms: f64,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            hash,
            date_part('epoch', init_timestamp) * 1000 AS init_ts_ms,
            date_part('epoch', end_timestamp) * 1000 AS end_ts_ms,
            replaced_hashes,
            number_of_entities,
            date_part('epoch', generation_time) * 1000 AS gen_ts_ms
        FROM snapshots
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SnapshotMetadata {
            hash: r.hash,
            time_range: TimeRange {
                init_timestamp: r.init_ts_ms,
                end_timestamp: r.end_ts_ms,
            },
            replaced_snapshot_hashes: r.replaced_hashes,
            number_of_entities: r.number_of_entities,
            generation_timestamp: r.gen_ts_ms,
        })
        .collect())
}

pub async fn delete_snapshot_by_time_range(
    pool: &PgPool,
    time_range: TimeRange,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM snapshots
        WHERE init_timestamp = to_timestamp($1 / 1000.0)
          AND end_timestamp = to_timestamp($2 / 1000.0)
        "#,
    )
    .bind(time_range.init_timestamp)
    .bind(time_range.end_timestamp)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_all_snapshot_hashes(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT DISTINCT hash FROM snapshots WHERE hash IS NOT NULL")
            .fetch_all(pool)
            .await?;

    Ok(rows.into_iter().filter_map(|r| r.0).collect())
}

pub async fn save_processed_snapshot(
    pool: &PgPool,
    hash: &str,
    process_timestamp_ms: f64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO processed_snapshots (hash, process_time)
        VALUES ($1, to_timestamp($2 / 1000.0))
        RETURNING hash
        "#,
    )
    .bind(hash)
    .bind(process_timestamp_ms)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_processed_snapshots(
    pool: &PgPool,
    hashes: &[String],
) -> Result<HashSet<String>, sqlx::Error> {
    if hashes.is_empty() {
        return Ok(HashSet::new());
    }

    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT hash FROM processed_snapshots WHERE hash = ANY($1)")
            .bind(hashes)
            .fetch_all(pool)
            .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}
