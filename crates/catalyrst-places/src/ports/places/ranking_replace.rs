use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Serialize;
use sqlx::{PgConnection, Row};

use crate::http::errors::ApiError;

use super::component::PlacesComponent;
use super::query::{EXCLUDE_FROM_RANKING_SQL, RANKING_IS_SET_SQL};

/// One destination's ranking for a single run of the automated score, already
/// routed to its own leg: a place is addressed by its catalogue id, a world by
/// its name or id, the way the caller's export names it.
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

// The buckets are exclusive so the caller's skip counts add up to what it
// sent: a destination that is both curated and a world is reported once, under
// curation, because that is the half a human would act on.
//
// `world` splits one table into the two upstream keeps apart, so a world row
// addressed as a place is the same category error there and here: browse
// orders worlds by their own leg and never reads a ranking written onto one.
// A destination this node serves itself lives in place_world_local, outside
// every statement below, so an id that resolves only there is named by the
// export and unwritable here -- reported as world-backed rather than missing,
// since the id does name a world.
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

// A world is addressed by name and stored under a row id that need not be the
// name, so the write travels with the resolved id while every report names the
// id the caller sent. A world served out of place_world_local resolves through
// the catalogue but not through this leg; upstream's result has no slot for
// "found and unwritable", so it lands in `missing` rather than growing the
// wire shape -- the same gap the single-destination ranking routes answer 503
// for.
//
// The second return names every submitted id that resolved onto a row another
// id in the same run had already claimed. Upstream matches `worlds.id`
// exactly, so its duplicate check over the raw ids is complete; ours resolves
// `lower(world_name)` the way find_world_by_id does, and two casings of one
// name would otherwise both reach the write with different numbers and let the
// UPDATE pick a winner. The caller sent two rankings for one destination
// either way, which is the run its own export was meant to refuse. Upstream
// answers 201 for that payload -- its `worlds.id` IS the lowercased name, so
// the other casing simply does not match and is reported in skipped_missing --
// and we refuse the whole run instead, which is an upstream-observable status
// divergence taken deliberately.
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

// Curation is protected by the predicate rather than by the caller: the route
// classifies first, so these conditions are unreachable through HTTP, but a
// clear-what-is-missing pass is dangerous enough that the guarantee belongs in
// the same statement as the write.
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

// Every automated ranking the run did not name is cleared, which is what makes
// the export authoritative for the whole uncurated population instead of only
// for the rows it sends: a per-row write can say what ranks, never what
// stopped ranking, so a destination that qualified once would keep its number
// forever.
//
// Upstream's places clear carries no `world` guard, so a world-backed place
// losing a stale ranking counts under places.cleared there. One table holds
// both legs here, and a world=true row is the same row on either side of that
// guard, so the same destination is swept by the worlds leg and counted under
// worlds.cleared instead. The set of rows zeroed is identical; only which of
// the two wire counters names them differs, and dropping the guard would move
// every genuine world into places.cleared, which is worse.
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
    /// Replace the whole automated ranking set in one transaction.
    ///
    /// Both halves share the transaction because a clear followed by a separate
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
