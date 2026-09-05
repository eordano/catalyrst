use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use catalyrst_types::duration_fmt::fmt_elapsed;
use catalyrst_worlds::config::Config;
use catalyrst_worlds::contents_temp;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::Semaphore;

const ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";

struct Args {
    jobs: usize,
    limit: Option<usize>,
    names: Vec<String>,
    force: bool,
}

fn parse_args() -> Args {
    let mut jobs = 32usize;
    let mut limit = None;
    let mut names = Vec::new();
    let mut force = false;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-j" | "--jobs" => {
                i += 1;
                jobs = argv.get(i).and_then(|s| s.parse().ok()).unwrap_or(32);
            }
            "--limit" => {
                i += 1;
                limit = argv.get(i).and_then(|s| s.parse().ok());
            }
            "--name" => {
                i += 1;
                match argv.get(i) {
                    Some(n) => names.push(n.clone()),
                    None => {
                        eprintln!("--name needs a world name");
                        std::process::exit(2);
                    }
                }
            }
            "--force" => force = true,
            other => {
                eprintln!("unknown arg {other:?} (usage: worlds-mirror [-j N] [--limit N] [--force] [--name world.dcl.eth ...])");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    // Overwriting a world someone deployed here destroys signed content, so it is
    // only ever done to a world named on the command line: an index-wide run with
    // --force would silently replace every local publish on the node.
    if force && names.is_empty() {
        eprintln!(
            "--force requires at least one --name: it overwrites a world published to this node, \
             which is not something to do across a whole index run"
        );
        std::process::exit(2);
    }
    Args {
        jobs,
        limit,
        names,
        force,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let cfg = Config::from_env()?;
    let upstream = cfg.contents_upstream_url.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "worlds-mirror needs CONTENTS_UPSTREAM_URL \u{2014} it mirrors content from another \
             worlds server and has nothing to do without one. There is no default."
        )
    })?;
    let contents_dir = cfg.contents_dir.clone();
    tokio::fs::create_dir_all(&contents_dir).await.ok();
    contents_temp::spawn_reaper(
        contents_dir.clone(),
        contents_temp::reap_grace(
            cfg.multipart_upload_timeout_ms,
            cfg.deployment_processing_timeout_ms,
        ),
    );

    let pool = PgPoolOptions::new()
        .max_connections((args.jobs as u32 + 4).min(32))
        .connect(&cfg.database_url)
        .await
        .context("connecting worlds DB")?;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("catalyrst-worlds-mirror/1.0")
        .build()?;

    let mut names: Vec<String> = if args.names.is_empty() {
        let index: Value = http
            .get(format!("{upstream}/index"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        index["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|w| {
                        w["scenes"]
                            .as_array()
                            .map(|s| !s.is_empty())
                            .unwrap_or(false)
                    })
                    .filter_map(|w| w["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        args.names.clone()
    };
    if let Some(n) = args.limit {
        names.truncate(n);
    }
    println!(
        "{}: {} worlds; upstream {upstream}; content dir {}",
        if args.names.is_empty() {
            "index"
        } else {
            "targets"
        },
        names.len(),
        contents_dir.display()
    );

    let total = names.len();
    let sem = Arc::new(Semaphore::new(args.jobs));
    let db_lock = Arc::new(tokio::sync::Mutex::new(()));
    let synced = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
    let refused = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let scenes_total = Arc::new(AtomicUsize::new(0));
    let blobs = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let t0 = Instant::now();

    let mut set = tokio::task::JoinSet::new();
    for name in names {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let (http, pool, upstream, contents_dir, db_lock) = (
            http.clone(),
            pool.clone(),
            upstream.clone(),
            contents_dir.clone(),
            db_lock.clone(),
        );
        let (synced, skipped, refused, failed, scenes_total, blobs, done) = (
            synced.clone(),
            skipped.clone(),
            refused.clone(),
            failed.clone(),
            scenes_total.clone(),
            blobs.clone(),
            done.clone(),
        );
        let force = args.force;
        set.spawn(async move {
            let _permit = permit;
            match mirror_world(&http, &pool, &db_lock, &upstream, &contents_dir, &name, force).await
            {
                Ok(Outcome::Synced(stats)) => {
                    synced.fetch_add(1, Ordering::Relaxed);
                    scenes_total.fetch_add(stats.scenes, Ordering::Relaxed);
                    blobs.fetch_add(stats.new_blobs, Ordering::Relaxed);
                }
                Ok(Outcome::RefusedLocalPublish { deployer }) => {
                    refused.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "!! REFUSED {name}: already published to this node by {deployer}. \
                         Mirroring would replace that signed deployment with a copy of the \
                         upstream world. Re-run with --name {name} --force to overwrite it."
                    );
                }
                Ok(Outcome::NothingUpstream(why)) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    eprintln!("skip {name}: {why}");
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    eprintln!("FAILED {name}: {e:#}");
                }
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 25 == 0 {
                let rate = d as f64 / t0.elapsed().as_secs_f64().max(1e-9);
                println!(
                    "  [{d}/{total}] synced={} skipped={} refused={} failed={} scenes={} new_blobs={}  {rate:.1} world/s",
                    synced.load(Ordering::Relaxed),
                    skipped.load(Ordering::Relaxed),
                    refused.load(Ordering::Relaxed),
                    failed.load(Ordering::Relaxed),
                    scenes_total.load(Ordering::Relaxed),
                    blobs.load(Ordering::Relaxed),
                );
            }
        });
    }
    while set.join_next().await.is_some() {}

    let refused_total = refused.load(Ordering::Relaxed);
    let failed_total = failed.load(Ordering::Relaxed);
    println!(
        "DONE: synced={} skipped={} refused={refused_total} failed={failed_total} scenes={} new_blobs={} in {}",
        synced.load(Ordering::Relaxed),
        skipped.load(Ordering::Relaxed),
        scenes_total.load(Ordering::Relaxed),
        blobs.load(Ordering::Relaxed),
        fmt_elapsed(t0.elapsed()),
    );
    if refused_total > 0 {
        eprintln!();
        eprintln!("!! {refused_total} world(s) were NOT mirrored because they are already published to this node.");
        eprintln!("!! Nothing was changed for them. The REFUSED lines above name each one.");
        eprintln!("!! To replace one with the upstream copy: worlds-mirror --name <world> --force");
    }
    Ok(())
}

struct WorldStats {
    scenes: usize,
    new_blobs: usize,
}

/// A world this run did not mirror is not automatically a failure, and the two
/// must not read alike: refusing to overwrite a local publish is the tool
/// working, while an unreachable upstream is the tool broken. Collapsing both
/// into one `skipped` counter is what makes a correct refusal look like a bug.
enum Outcome {
    Synced(WorldStats),
    RefusedLocalPublish { deployer: String },
    NothingUpstream(&'static str),
}

async fn mirror_world(
    http: &reqwest::Client,
    pool: &PgPool,
    db_lock: &tokio::sync::Mutex<()>,
    upstream: &str,
    contents_dir: &Path,
    name: &str,
    force: bool,
) -> Result<Outcome> {
    let local_deployer: Option<String> = sqlx::query_scalar(
        "SELECT deployer FROM world_scenes WHERE world_name = $1 AND deployer <> $2 LIMIT 1",
    )
    .bind(name)
    .bind(ZERO_ADDR)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("checking local publish for {name}"))?;
    if let Some(deployer) = local_deployer {
        if !force {
            return Ok(Outcome::RefusedLocalPublish { deployer });
        }
        eprintln!("  {name}: --force given; replacing content published here by {deployer}");
    }

    let resp = http
        .get(format!("{upstream}/world/{name}/about"))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(Outcome::NothingUpstream(
            "upstream /about did not answer 2xx",
        ));
    }
    let about: Value = resp.json().await?;

    let refs = scene_refs(&about);
    if refs.is_empty() {
        return Ok(Outcome::NothingUpstream("upstream world has no scenes"));
    }

    let skybox_time = about["configurations"]["skybox"]["fixedHour"].as_i64();
    let single_player = about["comms"]["adapter"].as_str() == Some("fixed-adapter:offline:offline");

    // Where the origin server drops arrivals, which is not always the first
    // scene's base parcel -- an operator can move it, and the entity never
    // learns. Reading it from the entity instead is why a mirrored world used to
    // keep the wrong spawn no matter how often the mirror was re-run.
    let upstream_spawn = about["spawnCoordinates"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(String::from);

    let mut new_blobs = 0usize;
    let mut scene_base_spawn: Option<String> = None;
    let mut records: Vec<(String, Value, Vec<String>, i64)> = Vec::new();
    for (cid, base) in refs {
        let entity_bytes = fetch_blob(http, contents_dir, &base, &cid, &mut new_blobs).await?;
        let entity: Value =
            serde_json::from_slice(&entity_bytes).with_context(|| format!("parse entity {cid}"))?;

        let mut size: i64 = 0;
        if let Some(content) = entity["content"].as_array() {
            for c in content {
                if let Some(hash) = c["hash"].as_str() {
                    let bytes = fetch_blob(http, contents_dir, &base, hash, &mut new_blobs).await?;
                    size += bytes.len() as i64;
                }
            }
        }

        let parcels: Vec<String> = entity["pointers"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if scene_base_spawn.is_none() {
            scene_base_spawn = entity["metadata"]["scene"]["base"]
                .as_str()
                .map(String::from);
        }
        records.push((cid, entity, parcels, size));
    }

    let _db = db_lock.lock().await;
    let mut tx = pool.begin().await.context("begin tx")?;
    // Upstream's spawn wins when it publishes one -- that is what mirroring
    // means. With none, a spawn an operator set here is preserved and the
    // scene's base parcel is only the last resort, so re-running the mirror
    // stops reverting a deliberate local choice.
    sqlx::query(
        r#"INSERT INTO worlds (name, spawn_coordinates, skybox_time, single_player, blocked_since, updated_at)
           VALUES ($1, COALESCE($2, $5), $3, $4, NULL, now())
           ON CONFLICT (name) DO UPDATE
             SET spawn_coordinates = COALESCE($2, worlds.spawn_coordinates, $5),
                 skybox_time = EXCLUDED.skybox_time,
                 single_player = EXCLUDED.single_player,
                 settings_version = worlds.settings_version
                   + CASE WHEN (worlds.skybox_time, worlds.single_player)
                          IS DISTINCT FROM (EXCLUDED.skybox_time, EXCLUDED.single_player)
                          THEN 1 ELSE 0 END,
                 blocked_since = NULL, updated_at = now()"#,
    )
    .bind(name)
    .bind(&upstream_spawn)
    .bind(skybox_time.map(|t| t as i32))
    .bind(single_player)
    .bind(&scene_base_spawn)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("upsert world {name}"))?;

    for (cid, entity, parcels, size) in &records {
        sqlx::query(
            r#"INSERT INTO world_scenes
                 (world_name, entity_id, deployment_auth_chain, entity, deployer, parcels, size)
               VALUES ($1, $2, '[]'::json, $3, $4, $5, $6)
               ON CONFLICT (world_name, entity_id) DO UPDATE
                 SET entity = EXCLUDED.entity, parcels = EXCLUDED.parcels, size = EXCLUDED.size"#,
        )
        .bind(name)
        .bind(cid)
        .bind(entity)
        .bind(ZERO_ADDR)
        .bind(parcels)
        .bind(*size)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("upsert scene {cid}"))?;
    }
    tx.commit().await.context("commit tx")?;

    Ok(Outcome::Synced(WorldStats {
        scenes: records.len(),
        new_blobs,
    }))
}

fn scene_refs(about: &Value) -> Vec<(String, String)> {
    about["configurations"]["scenesUrn"]
        .as_array()
        .map(|urns| {
            urns.iter()
                .filter_map(|u| u.as_str())
                .map(|urn| {
                    let after = urn
                        .split_once("urn:decentraland:entity:")
                        .map(|(_, b)| b)
                        .unwrap_or(urn);
                    let (cid, query) = after.split_once('?').unwrap_or((after, ""));
                    let base = query
                        .split('&')
                        .find_map(|kv| kv.strip_prefix("baseUrl="))
                        .map(String::from)
                        .unwrap_or_default();
                    (cid.trim().to_string(), base)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn temp_name(hash: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(".{hash}.{}.{nonce}.part", std::process::id())
}

async fn fetch_blob(
    http: &reqwest::Client,
    contents_dir: &Path,
    base_url: &str,
    hash: &str,
    new_blobs: &mut usize,
) -> Result<Vec<u8>> {
    let dst = contents_dir.join(hash);
    if let Ok(b) = tokio::fs::read(&dst).await {
        return Ok(b);
    }
    let url = format!("{base_url}{hash}");
    let bytes = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    let tmp = contents_dir.join(temp_name(hash));
    tokio::fs::write(&tmp, &bytes).await?;
    if let Err(e) = tokio::fs::rename(&tmp, &dst).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e.into());
    }
    *new_blobs += 1;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{contents_temp, temp_name};

    #[test]
    fn mirror_temps_follow_the_reaper_convention() {
        let name = temp_name("bafkreiabc");
        assert!(contents_temp::is_temp_name(&name), "{name}");
        assert!(name.starts_with(".bafkreiabc."));
        assert_ne!(name, temp_name("bafkreiabc"));
    }
}
