use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Serialize;
use sqlx::{PgConnection, Row};

use crate::http::errors::ApiError;

use super::component::PlacesComponent;
use super::query::{EXCLUDE_FROM_RANKING_SQL, RANKING_IS_SET_SQL};

/// A place is addressed by its catalogue id, a world by its name or id, the way
/// the caller's export names it.
#[derive(Debug, Clone, PartialEq)]
pub struct RankingEntry {
    pub id: String,
    pub ranking: f64,
}

#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ReplaceRankingPlaces {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub applied: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub cleared: i64,
    pub skipped_curated: Vec<String>,
    pub skipped_world_backed: Vec<String>,
    pub skipped_missing: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ReplaceRankingWorlds {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub applied: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub cleared: i64,
    pub skipped_curated: Vec<String>,
    pub skipped_missing: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "places/"))]
pub struct ReplaceRankingResult {
    pub places: ReplaceRankingPlaces,
    pub worlds: ReplaceRankingWorlds,
}

pub(super) struct CurationRow {
    pub id: String,
    pub highlighted: bool,
    pub exclude_from_ranking: bool,
    pub world: bool,
}

pub(super) struct WorldMatch {
    pub submitted: String,
    pub row_id: String,
    pub highlighted: bool,
    pub exclude_from_ranking: bool,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct PlaceBuckets {
    pub writable: Vec<RankingEntry>,
    pub curated: Vec<String>,
    pub world_backed: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct WorldBuckets {
    pub writable: Vec<RankingEntry>,
    pub curated: Vec<String>,
    pub missing: Vec<String>,
}

pub(super) fn classify_places(
    entries: &[RankingEntry],
    rows: &[CurationRow],
    locally_served: &[String],
) -> PlaceBuckets {
    let found: HashMap<&str, &CurationRow> =
        rows.iter().map(|row| (row.id.as_str(), row)).collect();
    let local: HashSet<&str> = locally_served.iter().map(String::as_str).collect();
    let mut buckets = PlaceBuckets::default();
    for entry in entries {
        match found.get(entry.id.as_str()) {
            Some(row) if row.highlighted || row.exclude_from_ranking => {
                buckets.curated.push(entry.id.clone())
            }
            Some(row) if row.world => buckets.world_backed.push(entry.id.clone()),
            Some(_) => buckets.writable.push(entry.clone()),
            None if local.contains(entry.id.as_str()) => {
                buckets.world_backed.push(entry.id.clone())
            }
            None => buckets.missing.push(entry.id.clone()),
        }
    }
    buckets
}

pub(super) fn classify_worlds(
    entries: &[RankingEntry],
    matches: &[WorldMatch],
) -> (WorldBuckets, Vec<String>) {
    let mut found: HashMap<&str, &WorldMatch> = HashMap::new();
    for m in matches {
        found.entry(m.submitted.as_str()).or_insert(m);
    }
    let mut buckets = WorldBuckets::default();
    let mut claimed: HashSet<&str> = HashSet::new();
    let mut collisions: Vec<String> = Vec::new();
    for entry in entries {
        match found.get(entry.id.as_str()) {
            Some(m) if m.highlighted || m.exclude_from_ranking => {
                buckets.curated.push(entry.id.clone())
            }
            Some(m) => {
                if !claimed.insert(m.row_id.as_str()) {
                    collisions.push(entry.id.clone());
                    continue;
                }
                buckets.writable.push(RankingEntry {
                    id: m.row_id.clone(),
                    ranking: entry.ranking,
                })
            }
            None => buckets.missing.push(entry.id.clone()),
        }
    }
    (buckets, collisions)
}

fn place_curation_sql() -> &'static str {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        format!(
            "SELECT id, highlighted, world, {EXCLUDE_FROM_RANKING_SQL} AS exclude_from_ranking \
             FROM place WHERE id = ANY($1)"
        )
    });
    &SQL
}

const LOCALLY_SERVED_WORLD_IDS: &str =
    "SELECT id FROM place_world_local WHERE world IS TRUE AND id = ANY($1)";

fn world_resolution_sql() -> &'static str {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        format!(
            "SELECT s.submitted AS submitted, place.id AS id, place.highlighted AS highlighted, \
             {EXCLUDE_FROM_RANKING_SQL} AS exclude_from_ranking \
             FROM unnest($1::text[]) AS s(submitted) \
             JOIN place ON place.world IS TRUE \
             AND (place.id = s.submitted OR lower(place.world_name) = lower(s.submitted)) \
             ORDER BY s.submitted, place.id"
        )
    });
    &SQL
}

fn apply_sql(world: bool) -> String {
    let leg = if world { "TRUE" } else { "FALSE" };
    format!(
        "UPDATE place SET raw = jsonb_set(COALESCE(raw, '{{}}'::jsonb), '{{ranking}}', \
         to_jsonb(incoming.ranking), true) \
         FROM (SELECT unnest($1::text[]) AS id, unnest($2::float8[]) AS ranking) AS incoming \
         WHERE place.id = incoming.id AND place.world IS {leg} \
         AND place.highlighted IS FALSE AND {EXCLUDE_FROM_RANKING_SQL} IS FALSE"
    )
}

fn clear_sql(world: bool) -> String {
    let leg = if world { "TRUE" } else { "FALSE" };
    format!(
        "UPDATE place SET raw = jsonb_set(COALESCE(raw, '{{}}'::jsonb), '{{ranking}}', \
         '0'::jsonb, true) \
         WHERE world IS {leg} AND highlighted IS FALSE \
         AND {EXCLUDE_FROM_RANKING_SQL} IS FALSE AND {RANKING_IS_SET_SQL} \
         AND id <> ALL($1::text[])"
    )
}

async fn apply_rankings(
    tx: &mut PgConnection,
    sql: &str,
    entries: &[RankingEntry],
) -> Result<i64, ApiError> {
    if entries.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let rankings: Vec<f64> = entries.iter().map(|e| e.ranking).collect();
    let applied = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(&ids)
        .bind(&rankings)
        .execute(tx)
        .await?
        .rows_affected();
    Ok(applied as i64)
}

async fn clear_rankings(
    tx: &mut PgConnection,
    sql: &str,
    keep: &[String],
) -> Result<i64, ApiError> {
    let cleared = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(keep)
        .execute(tx)
        .await?
        .rows_affected();
    Ok(cleared as i64)
}

impl PlacesComponent {
    /// Both halves share one transaction because a clear followed by a separate
    /// write leaves a window where the entire uncurated population reads as
    /// unranked, and browse requests landing in it would see no order at all.
    pub async fn replace_ranking(
        &self,
        places: &[RankingEntry],
        worlds: &[RankingEntry],
    ) -> Result<ReplaceRankingResult, ApiError> {
        let writer = self.place_writer()?;
        let mut tx = writer.begin().await?;

        let place_ids: Vec<String> = places.iter().map(|e| e.id.clone()).collect();
        let mut curation: Vec<CurationRow> = Vec::new();
        let mut locally_served: Vec<String> = Vec::new();
        if !place_ids.is_empty() {
            curation = sqlx::query(sqlx::AssertSqlSafe(place_curation_sql()))
                .bind(&place_ids)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| CurationRow {
                    id: r.get::<String, _>("id"),
                    highlighted: r.get::<bool, _>("highlighted"),
                    exclude_from_ranking: r.get::<bool, _>("exclude_from_ranking"),
                    world: r.get::<bool, _>("world"),
                })
                .collect();
            locally_served = sqlx::query(LOCALLY_SERVED_WORLD_IDS)
                .bind(&place_ids)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| r.get::<String, _>("id"))
                .collect();
        }
        let place_buckets = classify_places(places, &curation, &locally_served);

        let world_ids: Vec<String> = worlds.iter().map(|e| e.id.clone()).collect();
        let mut matches: Vec<WorldMatch> = Vec::new();
        if !world_ids.is_empty() {
            matches = sqlx::query(sqlx::AssertSqlSafe(world_resolution_sql()))
                .bind(&world_ids)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| WorldMatch {
                    submitted: r.get::<String, _>("submitted"),
                    row_id: r.get::<String, _>("id"),
                    highlighted: r.get::<bool, _>("highlighted"),
                    exclude_from_ranking: r.get::<bool, _>("exclude_from_ranking"),
                })
                .collect();
        }
        let (world_buckets, collisions) = classify_worlds(worlds, &matches);
        if !collisions.is_empty() {
            tx.rollback().await?;
            return Err(ApiError::bad_request(format!(
                "The same destination appears more than once: {}",
                collisions
                    .iter()
                    .map(|id| format!("world:{id}"))
                    .collect::<Vec<String>>()
                    .join(", ")
            )));
        }

        let places_applied =
            apply_rankings(&mut tx, &apply_sql(false), &place_buckets.writable).await?;
        let places_keep: Vec<String> = place_buckets
            .writable
            .iter()
            .map(|e| e.id.clone())
            .collect();
        let places_cleared = clear_rankings(&mut tx, &clear_sql(false), &places_keep).await?;

        let worlds_applied =
            apply_rankings(&mut tx, &apply_sql(true), &world_buckets.writable).await?;
        let worlds_keep: Vec<String> = world_buckets
            .writable
            .iter()
            .map(|e| e.id.clone())
            .collect();
        let worlds_cleared = clear_rankings(&mut tx, &clear_sql(true), &worlds_keep).await?;

        tx.commit().await?;

        Ok(ReplaceRankingResult {
            places: ReplaceRankingPlaces {
                applied: places_applied,
                cleared: places_cleared,
                skipped_curated: place_buckets.curated,
                skipped_world_backed: place_buckets.world_backed,
                skipped_missing: place_buckets.missing,
            },
            worlds: ReplaceRankingWorlds {
                applied: worlds_applied,
                cleared: worlds_cleared,
                skipped_curated: world_buckets.curated,
                skipped_missing: world_buckets.missing,
            },
        })
    }
}
