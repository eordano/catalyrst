use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::admin::authorize_admin;
use crate::config::Config;
use crate::contents_temp::{is_temp_name, reap_grace};
use crate::http::ApiError;
use crate::AppState;

const AUTH_SUFFIX: &str = ".auth";

/// Floor on how long an unreferenced blob must sit untouched before it is collectable. It has to
/// outlast the widest gap between a writer storing a blob and the row that references it landing --
/// worlds-mirror fetches every blob of a world before its single commit -- because inside that gap
/// the file is live but invisible to the active set.
const MIN_ORPHAN_AGE: Duration = Duration::from_secs(3600);

pub fn auth_key(entity_id: &str) -> String {
    format!("{entity_id}{AUTH_SUFFIX}")
}

pub fn is_collectable_name(name: &str) -> bool {
    if is_temp_name(name) {
        return false;
    }
    match name.strip_suffix(AUTH_SUFFIX) {
        Some(stem) => super::contents::is_retrievable_content_key(stem),
        None => super::contents::is_retrievable_content_key(name),
    }
}

pub fn orphan_age_floor(cfg: &Config) -> Duration {
    reap_grace(
        cfg.multipart_upload_timeout_ms,
        cfg.deployment_processing_timeout_ms,
    )
    .max(MIN_ORPHAN_AGE)
}

fn collect_content_hashes(
    entity_id: &str,
    content: Option<&Value>,
    out: &mut HashSet<String>,
) -> Result<(), String> {
    let items = match content {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            return Err(format!(
                "scene {entity_id} stores a non-array entity.content"
            ))
        }
    };
    for item in items {
        match item.get("hash").and_then(Value::as_str) {
            Some(hash) if !hash.is_empty() => {
                out.insert(hash.to_string());
            }
            _ => {
                return Err(format!(
                    "scene {entity_id} has a content entry without a usable hash"
                ))
            }
        }
    }
    Ok(())
}

/// The caller deletes everything this set does not name, so a row it cannot read exactly aborts the
/// whole set instead of contributing nothing.
pub async fn active_content_keys(pool: &PgPool) -> Result<HashSet<String>, ApiError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;

    let scenes = sqlx::query(
        r#"SELECT entity_id,
                  jsonb_typeof(entity) AS entity_type,
                  entity -> 'content'  AS content
           FROM world_scenes"#,
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut keys: HashSet<String> = HashSet::with_capacity(scenes.len() * 4);
    for row in &scenes {
        let entity_id: String = row.try_get("entity_id")?;
        let entity_type: Option<String> = row.try_get("entity_type")?;
        if entity_type.as_deref() != Some("object") {
            return Err(ApiError::internal(format!(
                "scene {entity_id} stores an entity that is not a JSON object"
            )));
        }
        let content: Option<Value> = row.try_get("content")?;
        collect_content_hashes(&entity_id, content.as_ref(), &mut keys)
            .map_err(ApiError::internal)?;
        keys.insert(auth_key(&entity_id));
        keys.insert(entity_id);
    }

    let thumbnails =
        sqlx::query("SELECT thumbnail_hash FROM worlds WHERE thumbnail_hash IS NOT NULL")
            .fetch_all(&mut *tx)
            .await?;
    for row in &thumbnails {
        let hash: String = row.try_get("thumbnail_hash")?;
        if !hash.is_empty() {
            keys.insert(hash);
        }
    }

    tx.commit().await?;
    Ok(keys)
}

fn global_scene_entity_ids(urns: Option<&str>) -> Vec<String> {
    urns.map(|raw| {
        raw.split_whitespace()
            .filter_map(|urn| {
                urn.split_once("urn:decentraland:entity:")
                    .map(|(_, rest)| rest)
            })
            .map(|rest| rest.split('?').next().unwrap_or(rest).trim().to_string())
            .filter(|cid| !cid.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// Global scenes are referenced by configuration rather than by a row, so their blobs would look
/// unreferenced to a set built from the database alone.
async fn add_global_scene_keys(cfg: &Config, out: &mut HashSet<String>) -> Result<(), ApiError> {
    for entity_id in global_scene_entity_ids(cfg.global_scenes_urn.as_deref()) {
        out.insert(auth_key(&entity_id));
        let path = cfg.contents_dir.join(&entity_id);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                out.insert(entity_id);
                continue;
            }
            Err(e) => {
                return Err(ApiError::internal(format!(
                    "could not read global scene entity {entity_id}: {e}"
                )))
            }
        };
        let entity: Value = serde_json::from_slice(&bytes).map_err(|e| {
            ApiError::internal(format!(
                "global scene entity {entity_id} does not parse: {e}"
            ))
        })?;
        collect_content_hashes(&entity_id, entity.get("content"), out)
            .map_err(ApiError::internal)?;
        out.insert(entity_id);
    }
    Ok(())
}

pub struct CollectableScan {
    pub scanned: usize,
    pub aged: Vec<(String, PathBuf)>,
}

/// Any enumeration error aborts instead of returning what was read so far: a truncated listing is
/// indistinguishable from a smaller directory, and the caller deletes from it.
pub async fn scan_collectable(
    dir: &std::path::Path,
    min_age: Duration,
) -> std::io::Result<CollectableScan> {
    let now = SystemTime::now();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CollectableScan {
                scanned: 0,
                aged: Vec::new(),
            })
        }
        Err(e) => return Err(e),
    };

    let mut scanned = 0usize;
    let mut aged = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        scanned += 1;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !is_collectable_name(name) {
            continue;
        }
        let meta = match entry.metadata().await {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if !meta.is_file() {
            continue;
        }
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= min_age);
        if !old_enough {
            continue;
        }
        aged.push((name.to_string(), entry.path()));
    }
    Ok(CollectableScan { scanned, aged })
}

pub fn select_orphans(
    aged: Vec<(String, PathBuf)>,
    active: &HashSet<String>,
) -> Vec<(String, PathBuf)> {
    aged.into_iter()
        .filter(|(name, _)| !active.contains(name.as_str()))
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct GcQuery {
    #[serde(default)]
    pub dry_run: Option<bool>,
    #[serde(default)]
    pub min_age_secs: Option<u64>,
    #[serde(default)]
    pub allow_empty_active_set: Option<bool>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct GcResponse {
    pub message: String,
    pub dry_run: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub removed: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub failed: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub candidates: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub scanned: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub active_keys: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub min_age_seconds: u64,
}

#[utoipa::path(
    post,
    path = "/admin/gc",
    tag = "admin",
    params(
        ("dry_run" = Option<bool>, Query, description = "Report what would be collected without deleting anything"),
        ("min_age_secs" = Option<u64>, Query, description = "Raise the minimum age an unreferenced file must reach; values below the server floor are ignored"),
        ("allow_empty_active_set" = Option<bool>, Query, description = "Collect even when the database names no active content at all")
    ),
    responses(
        (status = 200, body = GcResponse),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn garbage_collect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<GcQuery>,
) -> Result<Json<GcResponse>, ApiError> {
    run_gc(state, headers, q).await
}

#[utoipa::path(
    post,
    path = "/gc",
    tag = "admin",
    params(
        ("dry_run" = Option<bool>, Query, description = "Report what would be collected without deleting anything"),
        ("min_age_secs" = Option<u64>, Query, description = "Raise the minimum age an unreferenced file must reach; values below the server floor are ignored"),
        ("allow_empty_active_set" = Option<bool>, Query, description = "Collect even when the database names no active content at all")
    ),
    responses(
        (status = 200, body = GcResponse),
        (status = 400, body = catalyrst_types::ApiErrorBody),
        (status = 403, body = catalyrst_types::ApiErrorBody),
        (status = 500, body = catalyrst_types::ApiErrorBody)
    )
)]
pub async fn garbage_collect_root(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<GcQuery>,
) -> Result<Json<GcResponse>, ApiError> {
    run_gc(state, headers, q).await
}

async fn run_gc(
    state: AppState,
    headers: HeaderMap,
    q: GcQuery,
) -> Result<Json<GcResponse>, ApiError> {
    authorize_admin(&state, &headers)?;

    let floor = orphan_age_floor(&state.cfg);
    let min_age = q
        .min_age_secs
        .map(Duration::from_secs)
        .unwrap_or(floor)
        .max(floor);
    let dry_run = q.dry_run.unwrap_or(false);

    // The listing is taken before the active set so that a deployment committing mid-run protects
    // its own files; the reverse order would let a just-committed scene lose its content.
    let scan = scan_collectable(&state.cfg.contents_dir, min_age)
        .await
        .map_err(|e| {
            tracing::error!(
                error = %e,
                dir = %state.cfg.contents_dir.display(),
                "garbage collection aborted: could not enumerate the contents directory"
            );
            ApiError::internal("Could not enumerate the contents directory; nothing was deleted")
        })?;

    let mut active = active_content_keys(state.worlds.pool()).await?;
    add_global_scene_keys(&state.cfg, &mut active).await?;

    let orphans = select_orphans(scan.aged, &active);

    if active.is_empty() && !orphans.is_empty() && !q.allow_empty_active_set.unwrap_or(false) {
        return Err(ApiError::bad_request(
            "Refusing to collect: no world names any active content, which usually means the \
             worlds database is empty or misconfigured. Re-run with allow_empty_active_set=true \
             to collect anyway.",
        ));
    }

    if dry_run {
        return Ok(Json(GcResponse {
            message: format!(
                "Garbage collection would remove {} unused keys.",
                orphans.len()
            ),
            dry_run: true,
            removed: 0,
            failed: 0,
            candidates: orphans.len(),
            scanned: scan.scanned,
            active_keys: active.len(),
            min_age_seconds: min_age.as_secs(),
        }));
    }

    let mut removed = 0usize;
    let mut failed = 0usize;
    for (name, path) in &orphans {
        match tokio::fs::remove_file(path).await {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    error = %e,
                    key = %name,
                    "garbage collection: could not remove unreferenced content"
                );
            }
        }
    }

    tracing::info!(
        removed,
        failed,
        candidates = orphans.len(),
        scanned = scan.scanned,
        active_keys = active.len(),
        min_age_secs = min_age.as_secs(),
        dir = %state.cfg.contents_dir.display(),
        "garbage collection finished"
    );

    Ok(Json(GcResponse {
        message: format!("Garbage collection removed {removed} unused keys."),
        dry_run: false,
        removed,
        failed,
        candidates: orphans.len(),
        scanned: scan.scanned,
        active_keys: active.len(),
        min_age_seconds: min_age.as_secs(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cid() -> String {
        format!("ba{}", "a".repeat(57))
    }

    fn sha256_hex() -> String {
        "b".repeat(64)
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("worlds-gc-{tag}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn only_content_addressed_names_are_collectable() {
        let cid = cid();
        assert!(is_collectable_name(&cid));
        assert!(is_collectable_name(&auth_key(&cid)));
        assert!(is_collectable_name(&sha256_hex()));

        assert!(!is_collectable_name(&format!(".{cid}.4242.17.part")));
        assert!(!is_collectable_name(&format!(".{cid}.part")));
        assert!(!is_collectable_name(".keep"));
        assert!(!is_collectable_name("README"));
        assert!(!is_collectable_name("index.html"));
        assert!(!is_collectable_name(".auth"));
        assert!(!is_collectable_name("bafkrei.auth"));
        assert!(!is_collectable_name(&format!("{cid}.auth.auth")));
    }

    #[test]
    fn orphan_age_floor_never_drops_below_an_hour() {
        assert!(MIN_ORPHAN_AGE >= reap_grace(300_000, 300_000));
        assert_eq!(
            reap_grace(1, 1).max(MIN_ORPHAN_AGE),
            Duration::from_secs(3600)
        );
        assert_eq!(
            reap_grace(3_600_000, 3_600_000).max(MIN_ORPHAN_AGE),
            Duration::from_secs(7200)
        );
    }

    #[test]
    fn content_hashes_fail_closed_on_anything_unreadable() {
        let mut out = HashSet::new();
        assert!(collect_content_hashes("e", None, &mut out).is_ok());
        assert!(collect_content_hashes("e", Some(&Value::Null), &mut out).is_ok());
        assert!(out.is_empty());

        assert!(collect_content_hashes(
            "e",
            Some(&json!([{ "file": "a.png", "hash": "h1" }, { "file": "b.png", "hash": "h2" }])),
            &mut out
        )
        .is_ok());
        assert_eq!(out.len(), 2);
        assert!(out.contains("h1") && out.contains("h2"));

        assert!(collect_content_hashes("e", Some(&json!({})), &mut out).is_err());
        assert!(collect_content_hashes("e", Some(&json!("nope")), &mut out).is_err());
        assert!(
            collect_content_hashes("e", Some(&json!([{ "file": "a.png" }])), &mut out).is_err()
        );
        assert!(collect_content_hashes("e", Some(&json!([{ "hash": "" }])), &mut out).is_err());
        assert!(collect_content_hashes("e", Some(&json!([{ "hash": 7 }])), &mut out).is_err());
    }

    #[test]
    fn global_scene_urns_yield_their_entity_ids() {
        let cid = cid();
        let cfg_urn = format!(
            "urn:decentraland:entity:{cid}?baseUrl=https://example.org/contents/ \
             urn:decentraland:entity:{cid}"
        );
        let ids = global_scene_entity_ids(Some(&cfg_urn));
        assert_eq!(ids, vec![cid.clone(), cid]);
        assert!(global_scene_entity_ids(None).is_empty());
        assert!(global_scene_entity_ids(Some("not-a-urn")).is_empty());
    }

    #[tokio::test]
    async fn scan_skips_temps_directories_and_unmanaged_names() {
        let dir = scratch_dir("scan");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let cid = cid();
        let thumb = sha256_hex();
        for name in [
            cid.clone(),
            auth_key(&cid),
            thumb.clone(),
            ".keep".to_string(),
            "README".to_string(),
            format!(".{cid}.4242.17.part"),
        ] {
            tokio::fs::write(dir.join(name), b"x").await.unwrap();
        }
        tokio::fs::create_dir_all(dir.join(sha256_hex().replace('b', "c")))
            .await
            .unwrap();

        let fresh = scan_collectable(&dir, Duration::from_secs(3600))
            .await
            .unwrap();
        assert_eq!(fresh.scanned, 7);
        assert!(fresh.aged.is_empty());

        let aged = scan_collectable(&dir, Duration::ZERO).await.unwrap();
        assert_eq!(aged.scanned, 7);
        let mut names: Vec<String> = aged.aged.iter().map(|(n, _)| n.clone()).collect();
        names.sort();
        let mut expected = vec![cid.clone(), auth_key(&cid), thumb];
        expected.sort();
        assert_eq!(names, expected);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn scan_tolerates_a_contents_dir_that_does_not_exist_yet() {
        let scan = scan_collectable(&scratch_dir("missing"), Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(scan.scanned, 0);
        assert!(scan.aged.is_empty());
    }

    #[test]
    fn orphans_are_exactly_the_aged_files_no_active_key_names() {
        let cid = cid();
        let thumb = sha256_hex();
        let aged = vec![
            (cid.clone(), PathBuf::from(&cid)),
            (auth_key(&cid), PathBuf::from(auth_key(&cid))),
            (thumb.clone(), PathBuf::from(&thumb)),
        ];
        let active: HashSet<String> = [cid.clone(), auth_key(&cid)].into_iter().collect();
        let orphans = select_orphans(aged, &active);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].0, thumb);
    }
}
