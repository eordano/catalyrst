use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Result;
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use tokio_util::sync::CancellationToken;

use crate::catalog::derive::{derive, DerivedPlace};
use crate::ports::places::EXCLUDE_FROM_RANKING_SQL;

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
            'ranking',              place.raw->'ranking',
            'exclude_from_ranking', place.raw->'exclude_from_ranking',
            'highlighted_image',    place.raw->'highlighted_image',
            'like_score',           place.raw->'like_score',
            'like_rate',            place.raw->'like_rate'
        )),
        fetched_at = now()
    RETURNING (xmax = 0) AS inserted
"#;

// The enabled Genesis City places overlapping the incoming parcels, minus the
// row just written: the superseded place is still here because PRUNE only
// runs once the whole pass is over. Built at runtime so the curation flag
// keeps its single definition in ports/places/query.rs.
fn overlapping_places_sql() -> &'static str {
    static SQL: LazyLock<String> = LazyLock::new(|| {
        format!(
            r#"
    SELECT id, highlighted, creator_address,
           NULLIF(raw->>'ranking', '')::float8 AS ranking,
           {EXCLUDE_FROM_RANKING_SQL} AS exclude_from_ranking,
           raw->>'highlighted_image' AS highlighted_image
    FROM place
    WHERE id <> $1 AND disabled IS FALSE AND world IS FALSE
      AND raw->'positions' ?| $2::text[]
"#
        )
    });
    &SQL
}

// Written verbatim, nulls included: upstream's insert carries the
// predecessor's ranking and banner as they are, so an unranked highlighted
// predecessor leaves the new row unranked rather than at the derived default.
const INHERIT_CURATION: &str = r#"
    UPDATE place
    SET highlighted = $2,
        raw = raw || jsonb_build_object(
            'ranking',              $3::float8,
            'highlighted_image',    $4::text,
            'exclude_from_ranking', $5::boolean
        )
    WHERE id = $1
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

// A redeployment with a different base parcel lands on a new row (the id is
// derived from the base), so without this the highlighted flag and the
// hand-set ranking silently reset to the defaults. Inheriting is deliberately
// narrow: exactly one curated predecessor, published by the same known
// creator, or nothing -- a deployment on someone else's parcels is a takeover
// and must not promote an uncurated scene into the highlighted shelf.
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

// One transaction: a fresh row must never become visible at the default
// curation while its predecessor's is still being copied over.
async fn upsert(places: &PgPool, p: &DerivedPlace) -> Result<()> {
    let mut tx = places.begin().await?;
    let row = sqlx::query(UPSERT)
        .bind(&p.id)
        .bind(&p.base_position)
        .bind(&p.title)
        .bind(&p.description)
        .bind(&p.creator_address)
        .bind(&p.content_rating)
        .bind(p.deployed_at)
        .bind(&p.raw)
        .fetch_one(&mut *tx)
        .await?;
    let inserted: bool = row.try_get("inserted").unwrap_or(false);
    let positions: Vec<String> = p.raw["positions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !inserted || positions.is_empty() {
        tx.commit().await?;
        return Ok(());
    }
    let candidates: Vec<CurationCandidate> = sqlx::query(overlapping_places_sql())
        .bind(&p.id)
        .bind(&positions)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| CurationCandidate {
            creator_address: r.try_get("creator_address").unwrap_or(None),
            curation: Curation {
                highlighted: r.try_get("highlighted").unwrap_or(false),
                ranking: r.try_get("ranking").unwrap_or(None),
                exclude_from_ranking: r.try_get("exclude_from_ranking").unwrap_or(false),
                highlighted_image: r.try_get("highlighted_image").unwrap_or(None),
            },
        })
        .collect();
    if let Some(curation) = inherited_curation(&candidates, p.creator_address.as_deref()) {
        sqlx::query(INHERIT_CURATION)
            .bind(&p.id)
            .bind(curation.highlighted)
            .bind(curation.ranking)
            .bind(curation.highlighted_image)
            .bind(curation.exclude_from_ranking)
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
