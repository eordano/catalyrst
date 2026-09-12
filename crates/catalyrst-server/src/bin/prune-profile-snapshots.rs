//! Removes profile snapshot blobs no shipped client reads.
//!
//! A profile deploys up to four rendered images; only `face256.png` and `body.png` are ever read. The
//! unity and bevy explorers take those two straight from `avatar.snapshots` as opaque URLs, neither
//! reads `face` or `face128`, and the lambdas path rewrites `snapshots` to entity-addressed URLs
//! before any client sees it, so the stored hashes for the unread keys are never handed out. ADR-290
//! went further: the current client stops uploading snapshot content entirely.
//!
//! `face.png` is an independent 512x512 render, not a resample (`face256` is 256x256, `face128` is
//! 128x128), so nothing can regenerate it -- dead weight rather than a derivable cache.
//!
//! Two invariants this tool will not cross:
//!
//! 1. A content hash may be referenced by more than one file, so only hashes whose every live
//!    reference is a target key are eligible; on this corpus that excluded 112 shared blobs out of
//!    ~1.73M.
//! 2. Every eligible hash is written to a manifest BEFORE anything is removed. The blobs are
//!    content-addressed and still served by public peers, so the manifest is a recovery list, not
//!    just an audit trail.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tokio::io::AsyncWriteExt;

const DEFAULT_KEYS: &[&str] = &["face.png", "face128.png", "./face.png", "./face128.png"];

struct Args {
    database_url: String,
    storage_root: String,
    manifest: PathBuf,
    keys: Vec<String>,
    apply: bool,
    limit: Option<i64>,
}

fn usage() -> ! {
    eprintln!(
        "prune-profile-snapshots -- drop profile snapshot blobs no client reads\n\
         \n\
         Dry run by default: it reports what it would remove and writes the manifest,\n\
         but deletes nothing until --apply.\n\
         \n\
           --database-url URL   content DB (default $DATABASE_URL)\n\
           --storage-root DIR   content store root, the dir holding the shard dirs\n\
                               (default $STORAGE_ROOT_FOLDER/contents)\n\
           --manifest FILE      where the eligible hashes are recorded (default\n\
                               ./prune-profile-snapshots.manifest)\n\
           --keys a,b           snapshot keys to target (default face.png,face128.png\n\
                               and their ./ variants)\n\
           --limit N            stop after N eligible hashes (for a bounded trial)\n\
           --apply              actually delete; without it nothing is removed\n"
    );
    std::process::exit(2)
}

fn parse_args() -> Args {
    let mut a = Args {
        database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
        storage_root: std::env::var("STORAGE_ROOT_FOLDER")
            .map(|r| format!("{r}/contents"))
            .unwrap_or_default(),
        manifest: PathBuf::from("./prune-profile-snapshots.manifest"),
        keys: DEFAULT_KEYS.iter().map(|s| s.to_string()).collect(),
        apply: false,
        limit: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--database-url" => a.database_url = it.next().unwrap_or_else(|| usage()),
            "--storage-root" => a.storage_root = it.next().unwrap_or_else(|| usage()),
            "--manifest" => a.manifest = PathBuf::from(it.next().unwrap_or_else(|| usage())),
            "--keys" => {
                a.keys = it
                    .next()
                    .unwrap_or_else(|| usage())
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--limit" => {
                a.limit = Some(
                    it.next()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or_else(|| usage()),
                )
            }
            "--apply" => a.apply = true,
            "-h" | "--help" => usage(),
            _ => usage(),
        }
    }
    if a.database_url.is_empty() || a.storage_root.is_empty() {
        usage()
    }
    a
}

/// Hashes whose every live reference is one of `keys`. The NOT EXISTS is the safety half: a hash any
/// other key or entity type still points at stays, however many target-key references it also has.
async fn eligible_hashes(
    pool: &sqlx::PgPool,
    keys: &[String],
    limit: Option<i64>,
) -> Result<Vec<String>> {
    let sql = "
        WITH target AS (
            SELECT DISTINCT cf.content_hash
            FROM content_files cf
            JOIN deployments d ON d.id = cf.deployment
            WHERE d.entity_type = 'profile'
              AND d.deleter_deployment IS NULL
              AND cf.key = ANY($1)
        )
        SELECT t.content_hash
        FROM target t
        WHERE NOT EXISTS (
            SELECT 1
            FROM content_files o
            JOIN deployments od ON od.id = o.deployment
            WHERE o.content_hash = t.content_hash
              AND od.deleter_deployment IS NULL
              AND o.key <> ALL($1)
        )
        ORDER BY t.content_hash
        LIMIT $2";
    let rows = sqlx::query(sql)
        .bind(keys)
        .bind(limit.unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .context("selecting eligible hashes")?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>(0)).collect())
}

fn blob_path(root: &Path, hash: &str) -> PathBuf {
    root.join(catalyrst_storage::hex_prefix(hash)).join(hash)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let root = PathBuf::from(&args.storage_root);
    anyhow::ensure!(
        root.is_dir(),
        "storage root {} is not a directory",
        root.display()
    );

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&args.database_url)
        .await
        .context("connecting to the content database")?;

    eprintln!("prune-profile-snapshots: keys = {:?}", args.keys);
    let hashes = eligible_hashes(&pool, &args.keys, args.limit).await?;
    eprintln!(
        "eligible hashes (no other live reference): {}",
        hashes.len()
    );

    let mut manifest = tokio::fs::File::create(&args.manifest)
        .await
        .with_context(|| format!("creating manifest {}", args.manifest.display()))?;
    for h in &hashes {
        manifest.write_all(h.as_bytes()).await?;
        manifest.write_all(b"\n").await?;
    }
    manifest.sync_all().await?;
    eprintln!("manifest written: {}", args.manifest.display());

    let mut seen: HashSet<&str> = HashSet::new();
    let unique: Vec<&String> = hashes.iter().filter(|h| seen.insert(h.as_str())).collect();

    const CONCURRENCY: usize = 64;
    let present = AtomicUsize::new(0);
    let bytes = AtomicU64::new(0);
    let removed = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let total = unique.len();

    stream::iter(unique.into_iter().map(|h| {
        let p = blob_path(&root, h);
        let apply = args.apply;
        let present = &present;
        let bytes = &bytes;
        let removed = &removed;
        let done = &done;
        async move {
            if let Ok(meta) = tokio::fs::metadata(&p).await {
                if meta.is_file() {
                    present.fetch_add(1, Ordering::Relaxed);
                    bytes.fetch_add(meta.len(), Ordering::Relaxed);
                    if apply {
                        match tokio::fs::remove_file(&p).await {
                            Ok(()) => {
                                removed.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => eprintln!("warn: {}: {e}", p.display()),
                        }
                    }
                }
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200_000) {
                eprintln!("  .. {n}/{total}");
            }
        }
    }))
    .buffer_unordered(CONCURRENCY)
    .collect::<()>()
    .await;

    let present = present.load(Ordering::Relaxed);
    let removed = removed.load(Ordering::Relaxed);
    let bytes = bytes.load(Ordering::Relaxed);
    let gib = bytes as f64 / 1024.0 / 1024.0 / 1024.0;
    if args.apply {
        eprintln!("removed {removed} of {present} present blobs, {gib:.1} GiB reclaimed");
    } else {
        eprintln!("DRY RUN: {present} blobs present, {gib:.1} GiB would be reclaimed");
        eprintln!("re-run with --apply to delete them");
    }
    Ok(())
}
