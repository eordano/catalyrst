use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Clone, Copy, Debug)]
pub struct FetchError;

#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,

    pub name: Option<String>,

    pub base: [i32; 2],

    pub parcels: Vec<String>,

    pub thumbnail: Option<String>,

    pub creator: Option<String>,

    pub project_id: Option<String>,

    pub description: Option<String>,
}

#[derive(Deserialize)]
struct RawMeta {
    #[serde(default)]
    display: Option<Display>,
    #[serde(default)]
    scene: Option<SceneField>,
    #[serde(default)]
    contact: Option<Contact>,
    #[serde(default)]
    source: Option<Source>,
}

#[derive(Deserialize)]
struct Display {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "navmapThumbnail", default)]
    navmap_thumbnail: Option<String>,
}

#[derive(Deserialize)]
struct SceneField {
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    parcels: Vec<String>,
}

#[derive(Deserialize)]
struct Contact {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct Source {
    #[serde(rename = "projectId", default)]
    project_id: Option<String>,
}

struct TileEntry {
    scenes: Vec<Arc<Scene>>,
    at: Instant,
}

const MAX_CACHE_ENTRIES: usize = 4096;

type SceneRow = (String, serde_json::Value, Vec<String>, Option<String>);

const THUMBNAIL_HASH_SQL: &str = "CASE WHEN COALESCE(d.entity_metadata->'v', d.entity_metadata)->'display'->>'navmapThumbnail' LIKE 'http%'
                  THEN NULL
                  ELSE (SELECT cf.content_hash
                        FROM content_files cf
                        JOIN deployments d2 ON d2.id = cf.deployment
                        WHERE d2.entity_id = d.entity_id
                          AND cf.key = COALESCE(d.entity_metadata->'v', d.entity_metadata)->'display'->>'navmapThumbnail'
                        LIMIT 1) END";

/// One statement per miss set: each scene's matched pointers, its metadata and, when asked,
/// the navmap thumbnail's content hash for file-key thumbnails.
fn scene_sql(with_thumbnail_hash: bool) -> String {
    let thumbnail_hash = if with_thumbnail_hash {
        THUMBNAIL_HASH_SQL
    } else {
        "NULL::text"
    };
    format!(
        "SELECT d.entity_id, d.entity_metadata, p.pointers,
             {thumbnail_hash}
         FROM (SELECT entity_id, array_agg(DISTINCT pointer) AS pointers
               FROM active_pointers WHERE pointer = ANY($1) GROUP BY entity_id) p
         JOIN LATERAL (SELECT entity_id, entity_metadata FROM deployments d
                       WHERE d.entity_id = p.entity_id
                         AND d.entity_type = 'scene'
                         AND d.deleter_deployment IS NULL
                       LIMIT 1) d ON true
         ORDER BY d.entity_id"
    )
}

/// Scenes cached per tile, so a request only queries the tiles it has not seen inside the TTL.
pub struct ContentResolver {
    pool: Option<PgPool>,

    content_base_url: String,
    ttl: Duration,
    tiles: RwLock<HashMap<String, TileEntry>>,
}

impl ContentResolver {
    pub fn new(pool: Option<PgPool>, content_base_url: String, ttl_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            pool,
            content_base_url: content_base_url.trim_end_matches('/').to_string(),
            ttl: Duration::from_secs(ttl_secs.max(1)),
            tiles: RwLock::new(HashMap::new()),
        })
    }

    pub fn is_armed(&self) -> bool {
        self.pool.is_some()
    }

    pub async fn fetch_scenes(&self, tiles: &[String]) -> Result<Vec<Scene>, FetchError> {
        if tiles.is_empty() {
            return Ok(Vec::new());
        }
        let Some(pool) = self.pool.as_ref() else {
            return Ok(Vec::new());
        };

        let mut wanted = tiles.to_vec();
        wanted.sort();
        wanted.dedup();

        let mut found: HashMap<String, Arc<Scene>> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        {
            let guard = self.tiles.read();
            for tile in &wanted {
                match guard.get(tile) {
                    Some(entry) if entry.at.elapsed() < self.ttl => {
                        for scene in &entry.scenes {
                            found
                                .entry(scene.id.clone())
                                .or_insert_with(|| Arc::clone(scene));
                        }
                    }
                    _ => missing.push(tile.clone()),
                }
            }
        }

        if !missing.is_empty() {
            let fetched = self.query_scenes(pool, &missing).await?;
            let mut per_tile: HashMap<String, Vec<Arc<Scene>>> = missing
                .iter()
                .map(|tile| (tile.clone(), Vec::new()))
                .collect();
            for (pointers, scene) in fetched {
                let scene = Arc::new(scene);
                for pointer in pointers {
                    if let Some(list) = per_tile.get_mut(&pointer) {
                        list.push(Arc::clone(&scene));
                    }
                }
                found.entry(scene.id.clone()).or_insert(scene);
            }

            let ttl = self.ttl;
            let mut guard = self.tiles.write();
            guard.retain(|_, entry| entry.at.elapsed() < ttl);
            while guard.len() + per_tile.len() > MAX_CACHE_ENTRIES {
                let Some(oldest) = guard
                    .iter()
                    .min_by_key(|(_, e)| e.at)
                    .map(|(k, _)| k.clone())
                else {
                    break;
                };
                guard.remove(&oldest);
            }
            let at = Instant::now();
            for (tile, scenes) in per_tile {
                guard.insert(tile, TileEntry { scenes, at });
            }
        }

        let mut out: Vec<Scene> = found.into_values().map(|s| (*s).clone()).collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn query_scenes(
        &self,
        pool: &PgPool,
        tiles: &[String],
    ) -> Result<Vec<(Vec<String>, Scene)>, FetchError> {
        let rows = match self.scene_rows(pool, tiles, true).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %e, "hot-scenes: scene query with thumbnails failed, retrying without");
                match self.scene_rows(pool, tiles, false).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        tracing::warn!(error = %e, "hot-scenes: scene query failed");
                        return Err(FetchError);
                    }
                }
            }
        };

        let mut out = Vec::with_capacity(rows.len());
        for (entity_id, raw, pointers, thumbnail_hash) in rows {
            let meta_val = raw.get("v").cloned().unwrap_or(raw);
            let meta: RawMeta = match serde_json::from_value(meta_val) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let Some(scene) = meta.scene.as_ref() else {
                continue;
            };
            let Some(base_str) = scene.base.as_deref() else {
                continue;
            };
            if scene.parcels.is_empty() {
                continue;
            }
            let base = parse_coord(base_str);
            let thumbnail = self.thumbnail_url(meta.display.as_ref(), thumbnail_hash);
            out.push((
                pointers,
                Scene {
                    id: entity_id,
                    name: meta.display.as_ref().and_then(|d| d.title.clone()),
                    base,
                    parcels: scene.parcels.clone(),
                    thumbnail,
                    creator: meta.contact.as_ref().and_then(|c| c.name.clone()),
                    project_id: meta.source.as_ref().and_then(|s| s.project_id.clone()),
                    description: meta.display.as_ref().and_then(|d| d.description.clone()),
                },
            ));
        }
        Ok(out)
    }

    /// Thumbnail hashes ride the scene statement; the caller retries without them on failure so
    /// a broken thumbnail lookup never hides scenes.
    async fn scene_rows(
        &self,
        pool: &PgPool,
        tiles: &[String],
        with_thumbnail_hash: bool,
    ) -> Result<Vec<SceneRow>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(scene_sql(with_thumbnail_hash)))
            .bind(tiles)
            .fetch_all(pool)
            .await
    }

    fn thumbnail_url(&self, display: Option<&Display>, hash: Option<String>) -> Option<String> {
        let thumbnail = display.and_then(|d| d.navmap_thumbnail.clone())?;
        if thumbnail.starts_with("http") {
            return Some(thumbnail);
        }
        hash.map(|hash| format!("{}/contents/{}", self.content_base_url, hash))
    }
}

pub fn parse_coord(s: &str) -> [i32; 2] {
    let mut it = s.split(',').map(|p| p.trim().parse::<i32>().unwrap_or(0));
    [it.next().unwrap_or(0), it.next().unwrap_or(0)]
}
