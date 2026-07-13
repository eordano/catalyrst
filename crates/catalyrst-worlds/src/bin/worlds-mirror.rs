use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use catalyrst_worlds::config::Config;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::Semaphore;

const ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";

struct Args {
    jobs: usize,
    limit: Option<usize>,
}

fn parse_args() -> Args {
    let mut jobs = 32usize;
    let mut limit = None;
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
            other => {
                eprintln!("unknown arg {other:?}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    Args { jobs, limit }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let cfg = Config::from_env()?;
    let upstream = cfg.contents_upstream_url.trim_end_matches('/').to_string();
    let contents_dir = cfg.contents_dir.clone();
    tokio::fs::create_dir_all(&contents_dir).await.ok();

    let pool = PgPoolOptions::new()
        .max_connections((args.jobs as u32 + 4).min(32))
        .connect(&cfg.database_url)
        .await
        .context("connecting worlds DB")?;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("catalyrst-worlds-mirror/1.0")
        .build()?;

    let index: Value = http
        .get(format!("{upstream}/index"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut names: Vec<String> = index["data"]
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
        .unwrap_or_default();
    if let Some(n) = args.limit {
        names.truncate(n);
    }
    println!(
        "index: {} deployed worlds; upstream {upstream}; content dir {}",
        names.len(),
        contents_dir.display()
    );

    let total = names.len();
    let sem = Arc::new(Semaphore::new(args.jobs));
    let db_lock = Arc::new(tokio::sync::Mutex::new(()));
    let synced = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
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
        let (synced, skipped, scenes_total, blobs, done) = (
            synced.clone(),
            skipped.clone(),
            scenes_total.clone(),
            blobs.clone(),
            done.clone(),
        );
        set.spawn(async move {
            let _permit = permit;
            match mirror_world(&http, &pool, &db_lock, &upstream, &contents_dir, &name).await {
                Ok(Some(stats)) => {
                    synced.fetch_add(1, Ordering::Relaxed);
                    scenes_total.fetch_add(stats.scenes, Ordering::Relaxed);
                    blobs.fetch_add(stats.new_blobs, Ordering::Relaxed);
                }
                Ok(None) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    eprintln!("skip {name}: {e:#}");
                }
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 25 == 0 {
                let rate = d as f64 / t0.elapsed().as_secs_f64().max(1e-9);
                println!(
                    "  [{d}/{total}] synced={} skipped={} scenes={} new_blobs={}  {rate:.1} world/s",
                    synced.load(Ordering::Relaxed),
                    skipped.load(Ordering::Relaxed),
                    scenes_total.load(Ordering::Relaxed),
                    blobs.load(Ordering::Relaxed),
                );
            }
        });
    }
    while set.join_next().await.is_some() {}

    println!(
        "DONE: synced={} skipped={} scenes={} new_blobs={} in {:.0}s",
        synced.load(Ordering::Relaxed),
        skipped.load(Ordering::Relaxed),
        scenes_total.load(Ordering::Relaxed),
        blobs.load(Ordering::Relaxed),
        t0.elapsed().as_secs_f64(),
    );
    Ok(())
}

struct WorldStats {
    scenes: usize,
    new_blobs: usize,
}

async fn mirror_world(
    http: &reqwest::Client,
    pool: &PgPool,
    db_lock: &tokio::sync::Mutex<()>,
    upstream: &str,
    contents_dir: &Path,
    name: &str,
) -> Result<Option<WorldStats>> {
    let locally_published: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM world_scenes WHERE world_name = $1 AND deployer <> $2)",
    )
    .bind(name)
    .bind(ZERO_ADDR)
    .fetch_one(pool)
    .await
    .with_context(|| format!("checking local publish for {name}"))?;
    if locally_published {
        return Ok(None);
    }

    let resp = http
        .get(format!("{upstream}/world/{name}/about"))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let about: Value = resp.json().await?;

    let refs = scene_refs(&about);
    if refs.is_empty() {
        return Ok(None);
    }

    let skybox_time = about["configurations"]["skybox"]["fixedHour"].as_i64();
    let single_player = about["comms"]["adapter"].as_str() == Some("fixed-adapter:offline:offline");

    let mut new_blobs = 0usize;
    let mut spawn: Option<String> = None;
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
        if spawn.is_none() {
            spawn = entity["metadata"]["scene"]["base"]
                .as_str()
                .map(String::from);
        }
        records.push((cid, entity, parcels, size));
    }

    let _db = db_lock.lock().await;
    let mut tx = pool.begin().await.context("begin tx")?;
    sqlx::query(
        r#"INSERT INTO worlds (name, spawn_coordinates, skybox_time, single_player, blocked_since, updated_at)
           VALUES ($1, $2, $3, $4, NULL, now())
           ON CONFLICT (name) DO UPDATE
             SET spawn_coordinates = EXCLUDED.spawn_coordinates,
                 skybox_time = EXCLUDED.skybox_time,
                 single_player = EXCLUDED.single_player,
                 blocked_since = NULL, updated_at = now()"#,
    )
    .bind(name)
    .bind(&spawn)
    .bind(skybox_time.map(|t| t as i32))
    .bind(single_player)
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

    Ok(Some(WorldStats {
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
    let tmp = contents_dir.join(format!(".{hash}.part"));
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, &dst).await?;
    *new_blobs += 1;
    Ok(bytes)
}
