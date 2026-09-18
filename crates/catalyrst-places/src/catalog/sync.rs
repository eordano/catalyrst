use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Result;
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use tokio_util::sync::CancellationToken;

use crate::catalog::derive::{derive, DerivedPlace};
use crate::ports::places::{raw_float8_sql, EXCLUDE_FROM_RANKING_SQL};

pub const PAGE: i64 = 1000;
/// deployments.id is a plain `integer` sequence, so the walk starts below every
/// row it can hold.
pub const BEFORE_FIRST_SCENE: i32 = i32::MIN;
const INTERVAL: Duration = Duration::from_secs(3600);

/// Keyset, never OFFSET: an undeployment sets deleter_deployment mid-scan and
/// shifts every later OFFSET window back a row, so a still-served scene goes
/// unvisited and PRUNE then deletes it on its stale fetched_at.
pub const SELECT_SCENES: &str = r#"
    SELECT
        d.id,
        d.deployer_address,
        d.entity_pointers,
        (d.entity_timestamp AT TIME ZONE 'UTC') AS deployed_at,
        (d.entity_metadata::jsonb) AS meta,
        (SELECT cf.content_hash FROM content_files cf
          WHERE cf.deployment = d.id
            AND cf.key = (d.entity_metadata::jsonb)->'v'->'display'->>'navmapThumbnail'
          LIMIT 1) AS thumbnail_hash
    FROM deployments d
    WHERE d.entity_type = 'scene' AND d.deleter_deployment IS NULL AND d.id > $2
    ORDER BY d.id
    LIMIT $1
"#;

/// Every raw key an operator writes outside this rebuild has to be named in the
/// preservation object: the pass rebuilds `raw` wholesale from the deployment,
/// so a key it does not carry forward is erased within the hour.
const UPSERT: &str = r#"
    INSERT INTO place
        (id, base_position, title, description, creator_address, content_rating,
         categories, likes, dislikes, favorites, deployed_at, disabled, highlighted,
         raw, fetched_at)
    SELECT u.id, u.base_position, u.title, u.description, u.creator_address, u.content_rating,
           '{}', 0, 0, 0, u.deployed_at, false, false, u.raw, now()
    FROM unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[],
                $7::timestamptz[], $8::jsonb[])
         AS u(id, base_position, title, description, creator_address, content_rating,
              deployed_at, raw)
    ON CONFLICT (id) DO UPDATE SET
        base_position   = EXCLUDED.base_position,
        title           = EXCLUDED.title,
        description     = EXCLUDED.description,
        creator_address = EXCLUDED.creator_address,
        deployed_at     = EXCLUDED.deployed_at,
        raw = EXCLUDED.raw || jsonb_strip_nulls(jsonb_build_object(
            'ranking',              place.raw->'ranking',
            'exclude_from_ranking', place.raw->'exclude_from_ranking',
            'highlighted_image',    place.raw->'highlighted_image',
            'like_score',           place.raw->'like_score',
            'like_rate',            place.raw->'like_rate',
            'created_at',           place.raw->'created_at',
            'disabled_at',          place.raw->'disabled_at',
            'disabled_reason',      place.raw->'disabled_reason'
        )),
        fetched_at = now()
    RETURNING id, (xmax = 0) AS inserted
"#;

/// Overlap candidates for every fresh row of a page at once: `$1` the fresh ids, `$2`
/// each one's positions as a jsonb array.
pub(crate) fn overlapping_places_sql() -> &'static str {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        let ranking = raw_float8_sql("ranking");
        format!(
            r#"
    SELECT f.id AS fresh_id, p.id, p.highlighted, p.creator_address,
           {ranking} AS ranking,
           {EXCLUDE_FROM_RANKING_SQL} AS exclude_from_ranking,
           raw->>'highlighted_image' AS highlighted_image
    FROM unnest($1::text[], $2::jsonb[]) AS f(id, positions)
    JOIN place p
      ON p.id <> f.id AND p.disabled IS FALSE AND p.world IS FALSE
     AND p.raw->'positions' ?| ARRAY(SELECT jsonb_array_elements_text(f.positions))
"#
        )
    });
    &SQL
}

const INHERIT_CURATION: &str = r#"
    UPDATE place
    SET highlighted = u.highlighted,
        raw = raw || jsonb_build_object(
            'ranking',              u.ranking,
            'highlighted_image',    u.highlighted_image,
            'exclude_from_ranking', u.exclude_from_ranking
        )
    FROM unnest($1::text[], $2::boolean[], $3::float8[], $4::text[], $5::boolean[])
         AS u(id, highlighted, ranking, highlighted_image, exclude_from_ranking)
    WHERE place.id = u.id
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Curation {
    pub highlighted: bool,
    pub ranking: Option<f64>,
    pub exclude_from_ranking: bool,
    pub highlighted_image: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CurationCandidate {
    pub creator_address: Option<String>,
    pub curation: Curation,
}

pub fn inherited_curation(
    candidates: &[CurationCandidate],
    creator: Option<&str>,
) -> Option<Curation> {
    let creator = creator.filter(|c| !c.is_empty())?;
    let mut curated = candidates.iter().filter(|c| {
        (c.curation.highlighted
            || c.curation.exclude_from_ranking
            || c.curation.ranking.unwrap_or(0.0) > 0.0)
            && c.creator_address.as_deref() == Some(creator)
    });
    let predecessor = curated.next()?;
    if curated.next().is_some() {
        return None;
    }
    Some(predecessor.curation.clone())
}

const PRUNE: &str = r#"
    DELETE FROM place WHERE raw->>'source' = 'content' AND fetched_at < $1
"#;

pub fn spawn(places: PgPool, content: PgPool, content_public_url: String) {
    spawn_periodic(
        "place-catalog-sync",
        INTERVAL,
        PeriodicCfg::default(),
        CancellationToken::new(),
        move || {
            let places = places.clone();
            let content = content.clone();
            let content_public_url = content_public_url.clone();
            async move {
                match run_once(&places, &content, &content_public_url).await {
                    Ok((derived, pruned)) => tracing::info!(
                        derived,
                        pruned,
                        "place catalog synced from content deployments"
                    ),
                    Err(e) => tracing::warn!(error = %e, "place catalog sync failed"),
                }
                Ok::<(), anyhow::Error>(())
            }
        },
    );
}

pub async fn fetch_scene_page(
    content: &PgPool,
    page: i64,
    after_id: i32,
) -> Result<Vec<sqlx::postgres::PgRow>> {
    Ok(sqlx::query(SELECT_SCENES)
        .bind(page)
        .bind(after_id)
        .fetch_all(content)
        .await?)
}

pub async fn run_once(
    places: &PgPool,
    content: &PgPool,
    content_public_url: &str,
) -> Result<(usize, u64)> {
    let started: DateTime<Utc> = sqlx::query_scalar("SELECT now()").fetch_one(places).await?;
    let mut last_id = BEFORE_FIRST_SCENE;
    let mut derived = 0usize;
    loop {
        let rows = fetch_scene_page(content, PAGE, last_id).await?;
        let fetched = rows.len() as i64;
        let mut page: Vec<DerivedPlace> = Vec::with_capacity(rows.len());
        for row in &rows {
            last_id = row.try_get("id")?;
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
                page.push(p);
                derived += 1;
            }
        }
        upsert_page(places, page).await?;
        if fetched < PAGE {
            break;
        }
    }
    let pruned = sqlx::query(PRUNE)
        .bind(started)
        .execute(places)
        .await?
        .rows_affected();
    Ok((derived, pruned))
}

fn positions_of(p: &DerivedPlace) -> Vec<String> {
    p.raw["positions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One transaction per page: a multi-row upsert, one overlap query for the fresh rows,
/// and one batched curation update. Later duplicates of an id win, as they did row by row.
async fn upsert_page(places: &PgPool, page: Vec<DerivedPlace>) -> Result<()> {
    let mut slot_of: HashMap<&str, usize> = HashMap::new();
    let mut order: Vec<usize> = Vec::with_capacity(page.len());
    for (i, p) in page.iter().enumerate() {
        match slot_of.get(p.id.as_str()) {
            Some(&slot) => order[slot] = i,
            None => {
                slot_of.insert(p.id.as_str(), order.len());
                order.push(i);
            }
        }
    }
    let rows: Vec<&DerivedPlace> = order.iter().map(|&i| &page[i]).collect();
    if rows.is_empty() {
        return Ok(());
    }
    let mut tx = places.begin().await?;
    let inserted: HashMap<String, bool> = sqlx::query(UPSERT)
        .bind(rows.iter().map(|p| p.id.clone()).collect::<Vec<_>>())
        .bind(
            rows.iter()
                .map(|p| p.base_position.clone())
                .collect::<Vec<_>>(),
        )
        .bind(rows.iter().map(|p| p.title.clone()).collect::<Vec<_>>())
        .bind(
            rows.iter()
                .map(|p| p.description.clone())
                .collect::<Vec<_>>(),
        )
        .bind(
            rows.iter()
                .map(|p| p.creator_address.clone())
                .collect::<Vec<_>>(),
        )
        .bind(
            rows.iter()
                .map(|p| p.content_rating.clone())
                .collect::<Vec<_>>(),
        )
        .bind(rows.iter().map(|p| p.deployed_at).collect::<Vec<_>>())
        .bind(rows.iter().map(|p| p.raw.clone()).collect::<Vec<_>>())
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| {
            (
                r.try_get::<String, _>("id").unwrap_or_default(),
                r.try_get::<bool, _>("inserted").unwrap_or(false),
            )
        })
        .collect();
    let fresh: Vec<(&DerivedPlace, Vec<String>)> = rows
        .iter()
        .filter(|p| inserted.get(&p.id).copied().unwrap_or(false))
        .map(|p| (*p, positions_of(p)))
        .filter(|(_, positions)| !positions.is_empty())
        .collect();
    if fresh.is_empty() {
        tx.commit().await?;
        return Ok(());
    }
    let mut candidates: HashMap<String, Vec<(String, CurationCandidate)>> = HashMap::new();
    let found = sqlx::query(overlapping_places_sql())
        .bind(fresh.iter().map(|(p, _)| p.id.clone()).collect::<Vec<_>>())
        .bind(
            fresh
                .iter()
                .map(|(_, positions)| Value::from(positions.clone()))
                .collect::<Vec<Value>>(),
        )
        .fetch_all(&mut *tx)
        .await?;
    for r in found {
        let fresh_id: String = r.try_get("fresh_id").unwrap_or_default();
        let id: String = r.try_get("id").unwrap_or_default();
        candidates.entry(fresh_id).or_default().push((
            id,
            CurationCandidate {
                creator_address: r.try_get("creator_address").unwrap_or(None),
                curation: Curation {
                    highlighted: r.try_get("highlighted").unwrap_or(false),
                    ranking: r.try_get("ranking").unwrap_or(None),
                    exclude_from_ranking: r.try_get("exclude_from_ranking").unwrap_or(false),
                    highlighted_image: r.try_get("highlighted_image").unwrap_or(None),
                },
            },
        ));
    }
    // Fresh rows earlier in the page that just inherited count as curated for later ones,
    // exactly as when each row was committed before the next was examined.
    let mut inherited: Vec<(String, Curation)> = Vec::new();
    for (p, _) in &fresh {
        let mine: Vec<CurationCandidate> = candidates
            .remove(&p.id)
            .unwrap_or_default()
            .into_iter()
            .map(|(id, mut c)| {
                if let Some((_, cur)) = inherited.iter().find(|(i, _)| *i == id) {
                    c.curation = cur.clone();
                }
                c
            })
            .collect();
        if let Some(curation) = inherited_curation(&mine, p.creator_address.as_deref()) {
            inherited.push((p.id.clone(), curation));
        }
    }
    if !inherited.is_empty() {
        sqlx::query(INHERIT_CURATION)
            .bind(
                inherited
                    .iter()
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(
                inherited
                    .iter()
                    .map(|(_, c)| c.highlighted)
                    .collect::<Vec<_>>(),
            )
            .bind(inherited.iter().map(|(_, c)| c.ranking).collect::<Vec<_>>())
            .bind(
                inherited
                    .iter()
                    .map(|(_, c)| c.highlighted_image.clone())
                    .collect::<Vec<_>>(),
            )
            .bind(
                inherited
                    .iter()
                    .map(|(_, c)| c.exclude_from_ranking)
                    .collect::<Vec<_>>(),
            )
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod curation_tests {
    use super::*;

    const CREATOR: &str = "0x4f7fe261619141ffa63fefee35bba886581292f4";

    fn banner() -> Curation {
        Curation {
            highlighted: true,
            ranking: Some(1900.0),
            exclude_from_ranking: false,
            highlighted_image: Some("/images/places/banner.jpg".to_string()),
        }
    }

    fn candidate(creator: &str, curation: Curation) -> CurationCandidate {
        CurationCandidate {
            creator_address: Some(creator.to_string()),
            curation,
        }
    }

    #[test]
    fn carries_curation_from_a_predecessor_by_the_same_creator() {
        let inherited = inherited_curation(&[candidate(CREATOR, banner())], Some(CREATOR));
        assert_eq!(inherited, Some(banner()));
    }

    #[test]
    fn a_ranked_but_unhighlighted_predecessor_counts_as_curated() {
        let ranked = Curation {
            highlighted: false,
            ranking: Some(12.0),
            exclude_from_ranking: false,
            highlighted_image: None,
        };
        let inherited = inherited_curation(&[candidate(CREATOR, ranked.clone())], Some(CREATOR));
        assert_eq!(inherited, Some(ranked));
    }

    #[test]
    fn an_excluded_but_unranked_predecessor_counts_as_curated() {
        let excluded = Curation {
            highlighted: false,
            ranking: Some(0.0),
            exclude_from_ranking: true,
            highlighted_image: None,
        };
        let inherited = inherited_curation(&[candidate(CREATOR, excluded.clone())], Some(CREATOR));
        assert_eq!(inherited, Some(excluded));
    }

    #[test]
    fn a_predecessor_by_another_creator_inherits_nothing() {
        let other = "0x0000000000000000000000000000000000000001";
        assert_eq!(
            inherited_curation(&[candidate(CREATOR, banner())], Some(other)),
            None
        );
    }

    #[test]
    fn more_than_one_curated_predecessor_bails_out() {
        let candidates = [candidate(CREATOR, banner()), candidate(CREATOR, banner())];
        assert_eq!(inherited_curation(&candidates, Some(CREATOR)), None);
    }

    #[test]
    fn uncurated_neighbours_do_not_count_towards_the_bail_out() {
        let plain = Curation {
            highlighted: false,
            ranking: Some(0.0),
            exclude_from_ranking: false,
            highlighted_image: None,
        };
        let candidates = [candidate(CREATOR, banner()), candidate(CREATOR, plain)];
        assert_eq!(
            inherited_curation(&candidates, Some(CREATOR)),
            Some(banner())
        );
    }

    #[test]
    fn a_deployment_without_a_known_creator_inherits_nothing() {
        assert_eq!(
            inherited_curation(&[candidate(CREATOR, banner())], None),
            None
        );
        assert_eq!(
            inherited_curation(&[candidate(CREATOR, banner())], Some("")),
            None
        );
    }
}
