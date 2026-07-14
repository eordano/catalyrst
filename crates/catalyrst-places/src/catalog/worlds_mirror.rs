use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

/// Upstream clamps `limit` to 100 (places getWorldListQuery).
const PAGE: i64 = 100;
/// A cycle aborts after this many consecutive failed pages so an unreachable
/// upstream cannot spin the offset forever.
const MAX_CONSECUTIVE_PAGE_FAILURES: u32 = 3;
/// Hard offset ceiling; the live catalog is ~1.6k worlds, so hitting this
/// means the upstream is misbehaving, not that the catalog grew 30x.
const MAX_OFFSET: i64 = 50_000;
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; catalyrst-places-worlds-mirror/1; +https://decentraland.org)";

// world / world_name never appear in this column list: on `place` they are
// GENERATED from raw (migrations/0003), so storing the upstream row verbatim
// as raw is what makes the row a world.
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

/// Worlds age out by omission: a complete pass stamps every world upstream
/// still lists with a fresh fetched_at, so a world row is deleted only after
/// two consecutive full passes fail to list it (the cutoff sits two mirror
/// intervals before the current pass, so one pagination miss never deletes a
/// live row). Only this mirror writes world rows into `place` -- the places
/// mirror and the content derive produce world=false rows, and locally-served
/// worlds live in place_world_local, which this DELETE never reaches -- so the
/// cutoff removes exactly the upstream-removed worlds.
const SWEEP: &str = "DELETE FROM place WHERE world IS TRUE AND fetched_at < $1";

/// The rows SWEEP would delete at a given cutoff, counted before deleting.
const SWEEP_CANDIDATES: &str = "SELECT count(*) FROM place WHERE world IS TRUE AND fetched_at < $1";

/// The mirror-owned rows the sweep can reach at all.
const MIRRORED_COUNT: &str = "SELECT count(*) FROM place WHERE world IS TRUE";

/// The sweep refuses to delete anything when its candidate set exceeds
/// max(SWEEP_FUSE_MIN_CANDIDATES, 20% of the mirror-owned rows): a
/// well-formed but truncated upstream response still reports complete, and
/// must not mass-delete the catalog.
const SWEEP_FUSE_MIN_CANDIDATES: i64 = 50;

#[derive(Debug, Default)]
pub struct CycleOutcome {
    pub upserted: usize,
    pub skipped_rows: usize,
    pub failed_rows: usize,
    pub failed_pages: usize,
    pub swept: u64,
    /// True only when every page fetched and every row upserted cleanly; the
    /// delete sweep is gated on it so a flaky cycle can never wipe rows it
    /// merely failed to see.
    pub complete: bool,
}

pub fn spawn(pool: PgPool, upstream_url: String, interval: Duration) {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "worlds mirror: http client build failed; disabled");
                return;
            }
        };
        loop {
            match run_cycle(&pool, &client, &upstream_url, interval).await {
                Ok(o) => tracing::info!(
                    upserted = o.upserted,
                    skipped_rows = o.skipped_rows,
                    failed_rows = o.failed_rows,
                    failed_pages = o.failed_pages,
                    swept = o.swept,
                    complete = o.complete,
                    "world catalog mirrored from upstream"
                ),
                Err(e) => tracing::warn!(error = %e, "world catalog mirror cycle failed"),
            }
            tokio::time::sleep(interval).await;
        }
    });
}

/// One full mirror pass: pages through the upstream /api/worlds catalog,
/// upserts every row, then sweeps world rows the upstream has stopped
/// serving. Fail-open per page and per row: an error logs, counts, and the
/// pass moves on.
pub async fn run_cycle(
    pool: &PgPool,
    client: &reqwest::Client,
    upstream: &str,
    interval: Duration,
) -> Result<CycleOutcome> {
    let base = upstream.trim_end_matches('/');
    let pass_start = Utc::now();
    let mut out = CycleOutcome {
        complete: true,
        ..Default::default()
    };
    let mut offset = 0i64;
    let mut consecutive_failures = 0u32;

    loop {
        if offset >= MAX_OFFSET {
            tracing::warn!(offset, "worlds mirror: offset ceiling reached; ending pass");
            out.complete = false;
            break;
        }
        let url = format!("{base}/api/worlds?limit={PAGE}&offset={offset}");
        match fetch_page(client, &url).await {
            Ok(worlds) => {
                consecutive_failures = 0;
                if worlds.is_empty() {
                    break;
                }
                let count = worlds.len() as i64;
                for world in &worlds {
                    match upsert_world(pool, world).await {
                        Ok(true) => out.upserted += 1,
                        Ok(false) => out.skipped_rows += 1,
                        Err(e) => {
                            out.failed_rows += 1;
                            out.complete = false;
                            let id = world.get("id").and_then(Value::as_str).unwrap_or("?");
                            tracing::warn!(error = %e, world_id = id, "worlds mirror: upsert failed; row skipped");
                        }
                    }
                }
                if count < PAGE {
                    break;
                }
                offset += PAGE;
            }
            Err(e) => {
                out.failed_pages += 1;
                out.complete = false;
                consecutive_failures += 1;
                tracing::warn!(error = %e, url = %url, "worlds mirror: page fetch failed; skipped");
                if consecutive_failures >= MAX_CONSECUTIVE_PAGE_FAILURES {
                    tracing::warn!(
                        consecutive_failures,
                        "worlds mirror: too many consecutive page failures; ending pass"
                    );
                    break;
                }
                offset += PAGE;
            }
        }
    }

    if out.complete && out.upserted > 0 {
        // A row is deleted only after two consecutive full passes fail to
        // list it: rows seen during pass N carry fetched_at >= that pass's
        // start, so a cutoff two intervals before this pass keeps anything
        // the previous pass listed -- one pagination miss never deletes.
        let grace = interval
            .checked_mul(2)
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(|| chrono::Duration::hours(2));
        let cutoff = pass_start - grace;
        match sweep_removed(pool, cutoff).await {
            Ok(SweepOutcome::Swept(n)) => out.swept = n,
            Ok(SweepOutcome::Refused { .. }) => {}
            Err(e) => {
                tracing::warn!(error = %e, "worlds mirror: sweep failed");
            }
        }
    }
    Ok(out)
}

async fn fetch_page(client: &reqwest::Client, url: &str) -> Result<Vec<Value>> {
    let body: Value = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_page(&body)
}

/// The upstream envelope is `{"total": N, "ok": true, "data": [world, ...]}`.
pub fn parse_page(body: &Value) -> Result<Vec<Value>> {
    if body.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow!("worlds upstream returned ok!=true"));
    }
    body.get("data")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow!("worlds upstream response has no data array"))
}

/// The indexed columns the serving queries filter and order on; every other
/// served field (image, created_at, updated_at, like_rate, like_score,
/// is_private, show_in_places, single_player, skybox_time, user_visits,
/// highlighted_image, ranking, contact_name, ...) is read from `raw`, which
/// stores the upstream row verbatim. Everything except the identity pair is
/// tolerant of being absent or null upstream, and unknown keys pass through
/// untouched inside raw.
#[derive(Debug, Deserialize)]
pub struct WorldFields {
    pub id: String,
    pub world_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub base_position: Option<String>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub likes: Option<i32>,
    #[serde(default)]
    pub dislikes: Option<i32>,
    #[serde(default)]
    pub favorites: Option<i32>,
    #[serde(default)]
    pub deployed_at: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub highlighted: Option<bool>,
}

/// Parses one upstream /api/worlds row into the typed column values plus the
/// raw JSON to store. A world is identified by its name (0003's invariant),
/// so a row without an id or a world_name is unservable and yields None --
/// skipped, not fatal. The stored raw always carries world=true: the row's
/// provenance is the worlds listing, and the generated `world` column
/// computes from that key.
pub fn extract_fields(world: &Value) -> Option<(WorldFields, Value)> {
    let typed: WorldFields = serde_json::from_value(world.clone()).ok()?;
    if typed.id.trim().is_empty() || typed.world_name.trim().is_empty() {
        return None;
    }
    let mut raw = world.clone();
    if raw.get("world").and_then(Value::as_bool) != Some(true) {
        raw["world"] = Value::Bool(true);
    }
    Some((typed, raw))
}

/// Ok(false) means the row was skipped (unservable), Ok(true) means upserted.
pub async fn upsert_world(pool: &PgPool, world: &Value) -> Result<bool> {
    let Some((w, raw)) = extract_fields(world) else {
        return Ok(false);
    };
    sqlx::query(UPSERT)
        .bind(&w.id)
        .bind(w.base_position.as_deref().unwrap_or("0,0"))
        .bind(w.title.as_deref().unwrap_or(""))
        .bind(w.description.as_deref().unwrap_or(""))
        .bind(
            w.owner
                .as_deref()
                .map(str::to_lowercase)
                .filter(|s| !s.is_empty()),
        )
        .bind(w.content_rating.as_deref())
        .bind(w.categories.as_deref().unwrap_or(&[]))
        .bind(w.likes.unwrap_or(0))
        .bind(w.dislikes.unwrap_or(0))
        .bind(w.favorites.unwrap_or(0))
        .bind(w.deployed_at.as_deref())
        .bind(w.disabled.unwrap_or(false))
        .bind(w.highlighted.unwrap_or(false))
        .bind(&raw)
        .execute(pool)
        .await?;
    Ok(true)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SweepOutcome {
    /// Rows deleted.
    Swept(u64),
    /// The volume fuse tripped: `candidates` rows were up for deletion out
    /// of `mirrored` mirror-owned rows, and nothing was deleted.
    Refused { candidates: i64, mirrored: i64 },
}

/// Deletes mirrored world rows last seen before `cutoff`. Refuses to delete
/// anything when the candidate set exceeds max(SWEEP_FUSE_MIN_CANDIDATES,
/// 20% of the mirror-owned rows).
pub async fn sweep_removed(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<SweepOutcome> {
    let candidates: i64 = sqlx::query_scalar(SWEEP_CANDIDATES)
        .bind(cutoff)
        .fetch_one(pool)
        .await?;
    let mirrored: i64 = sqlx::query_scalar(MIRRORED_COUNT).fetch_one(pool).await?;
    let threshold = SWEEP_FUSE_MIN_CANDIDATES.max(mirrored / 5);
    if candidates > threshold {
        tracing::warn!(
            candidates,
            mirrored,
            threshold,
            "worlds mirror: sweep would delete too many rows; skipping sweep"
        );
        return Ok(SweepOutcome::Refused {
            candidates,
            mirrored,
        });
    }
    let res = sqlx::query(SWEEP).bind(cutoff).execute(pool).await?;
    Ok(SweepOutcome::Swept(res.rows_affected()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Real captured response from places.decentraland.org/api/worlds.
    const WORLDS_PAGE: &str = include_str!("../../tests/fixtures/worlds_page.json");

    fn fixture() -> Value {
        serde_json::from_str(WORLDS_PAGE).expect("fixture is valid JSON")
    }

    #[test]
    fn fixture_page_is_a_real_upstream_envelope() {
        let body = fixture();
        assert!(body["total"].as_i64().unwrap() > 1000, "{}", body["total"]);
        let rows = parse_page(&body).expect("fixture envelope parses");
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn every_fixture_row_parses_with_identity_and_columns() {
        for row in parse_page(&fixture()).unwrap() {
            let (w, raw) = extract_fields(&row).expect("real upstream rows parse");
            assert!(!w.id.is_empty());
            assert!(!w.world_name.is_empty());
            assert_eq!(raw["world"], json!(true));
            assert_eq!(&raw["world_name"], &row["world_name"]);
        }
    }

    #[test]
    fn known_fixture_row_maps_field_for_field() {
        let (w, raw) = extract_fields(&parse_page(&fixture()).unwrap()[0]).expect("row 0 parses");
        assert_eq!(w.id, "metadynelabs.dcl.eth");
        assert_eq!(w.world_name, "MetadyneLabs.dcl.eth");
        assert_eq!(w.title.as_deref(), Some("Metadyne Labs - Rat Scape"));
        assert_eq!(
            w.owner.as_deref(),
            Some("0x6adf75e49bac21abab9adb9266d2cc6d90abd31a")
        );
        assert_eq!(w.content_rating.as_deref(), Some("RP"));
        assert_eq!(w.base_position.as_deref(), Some("0,0"));
        assert_eq!(w.categories.as_deref(), Some(&[][..]));
        assert_eq!(w.likes, Some(94));
        assert_eq!(w.dislikes, Some(4));
        assert_eq!(w.favorites, Some(3));
        assert_eq!(w.disabled, Some(false));
        assert_eq!(w.highlighted, Some(false));
        assert_eq!(w.deployed_at.as_deref(), Some("2024-08-08T11:50:58.807Z"));
        assert!(raw["image"]
            .as_str()
            .unwrap()
            .starts_with("https://worlds-content-server.decentraland.org/contents/"));
        assert_eq!(raw["like_score"], json!(0.85771024));
        assert_eq!(raw["created_at"], json!("2023-10-17T16:31:57.766Z"));
    }

    #[test]
    fn unused_and_unknown_fields_do_not_break_the_parse() {
        let (w, _) = extract_fields(&json!({
            "id": "a.dcl.eth",
            "world_name": "a.dcl.eth",
            "settings_version": null,
            "skybox_time": null,
            "some_future_field": {"nested": [1, 2, 3]},
            "likes": null,
            "categories": null
        }))
        .expect("nulls and unknown keys are tolerated");
        assert_eq!(w.likes, None);
        assert_eq!(w.categories, None);
        assert_eq!(w.title, None);
    }

    #[test]
    fn a_row_without_a_world_name_is_skipped() {
        assert!(extract_fields(&json!({"id": "x.dcl.eth"})).is_none());
        assert!(extract_fields(&json!({"id": "x.dcl.eth", "world_name": ""})).is_none());
        assert!(extract_fields(&json!({"id": "x.dcl.eth", "world_name": "   "})).is_none());
        assert!(extract_fields(&json!({"id": "", "world_name": "x.dcl.eth"})).is_none());
    }

    #[test]
    fn stored_raw_always_carries_the_world_flag() {
        let (_, raw) =
            extract_fields(&json!({"id": "a.dcl.eth", "world_name": "a.dcl.eth"})).unwrap();
        assert_eq!(raw["world"], json!(true));
        let (_, raw) =
            extract_fields(&json!({"id": "a.dcl.eth", "world_name": "a.dcl.eth", "world": false}))
                .unwrap();
        assert_eq!(raw["world"], json!(true));
    }

    #[test]
    fn envelope_rejects_ok_false_and_missing_data() {
        assert!(parse_page(&json!({"ok": false, "data": []})).is_err());
        assert!(parse_page(&json!({"ok": true})).is_err());
        assert!(parse_page(&json!({"data": []})).is_err());
    }
}
