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

struct CacheEntry {
    scenes: Vec<Scene>,
    at: Instant,
}

const MAX_CACHE_ENTRIES: usize = 512;

pub struct ContentResolver {
    pool: Option<PgPool>,

    content_base_url: String,
    ttl: Duration,
    cache: RwLock<HashMap<String, CacheEntry>>,
}

impl ContentResolver {
    pub fn new(pool: Option<PgPool>, content_base_url: String, ttl_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            pool,
            content_base_url: content_base_url.trim_end_matches('/').to_string(),
            ttl: Duration::from_secs(ttl_secs.max(1)),
            cache: RwLock::new(HashMap::new()),
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

        let mut key_tiles = tiles.to_vec();
        key_tiles.sort();
        key_tiles.dedup();
        let cache_key = key_tiles.join(";");

        {
            let guard = self.cache.read();
            if let Some(entry) = guard.get(&cache_key) {
                if entry.at.elapsed() < self.ttl {
                    return Ok(entry.scenes.clone());
                }
            }
        }

        let scenes = self.query_scenes(pool, &key_tiles).await?;

        {
            let ttl = self.ttl;
            let mut guard = self.cache.write();
            guard.retain(|_, entry| entry.at.elapsed() < ttl);
            while guard.len() >= MAX_CACHE_ENTRIES {
                let Some(oldest) = guard
                    .iter()
                    .min_by_key(|(_, e)| e.at)
                    .map(|(k, _)| k.clone())
                else {
                    break;
                };
                guard.remove(&oldest);
            }
            guard.insert(
                cache_key,
                CacheEntry {
                    scenes: scenes.clone(),
                    at: Instant::now(),
                },
            );
        }
        Ok(scenes)
    }

    async fn query_scenes(
        &self,
        pool: &PgPool,
        tiles: &[String],
    ) -> Result<Vec<Scene>, FetchError> {
        let rows: Vec<(String, serde_json::Value)> = match sqlx::query_as(
            r#"
            SELECT DISTINCT ON (d.entity_id) d.entity_id, d.entity_metadata
            FROM active_pointers ap
            JOIN deployments d ON d.entity_id = ap.entity_id
            WHERE ap.pointer = ANY($1)
              AND d.entity_type = 'scene'
              AND d.deleter_deployment IS NULL
            "#,
        )
        .bind(tiles)
        .fetch_all(pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "hot-scenes: scene query failed");
                return Err(FetchError);
            }
        };

        let mut out = Vec::with_capacity(rows.len());
        for (entity_id, raw) in rows {
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
            let thumbnail = self
                .calculate_thumbnail(&entity_id, meta.display.as_ref(), pool)
                .await;
            out.push(Scene {
                id: entity_id,
                name: meta.display.as_ref().and_then(|d| d.title.clone()),
                base,
                parcels: scene.parcels.clone(),
                thumbnail,
                creator: meta.contact.as_ref().and_then(|c| c.name.clone()),
                project_id: meta.source.as_ref().and_then(|s| s.project_id.clone()),
                description: meta.display.as_ref().and_then(|d| d.description.clone()),
            });
        }
        Ok(out)
    }

    async fn calculate_thumbnail(
        &self,
        entity_id: &str,
        display: Option<&Display>,
        pool: &PgPool,
    ) -> Option<String> {
        let thumbnail = display.and_then(|d| d.navmap_thumbnail.clone())?;
        if thumbnail.starts_with("http") {
            return Some(thumbnail);
        }

        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT cf.content_hash
            FROM content_files cf
            JOIN deployments d ON d.id = cf.deployment
            WHERE d.entity_id = $1 AND cf.key = $2
            LIMIT 1
            "#,
        )
        .bind(entity_id)
        .bind(&thumbnail)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        row.map(|(hash,)| format!("{}/contents/{}", self.content_base_url, hash))
    }
}

pub fn parse_coord(s: &str) -> [i32; 2] {
    let mut it = s.split(',').map(|p| p.trim().parse::<i32>().unwrap_or(0));
    [it.next().unwrap_or(0), it.next().unwrap_or(0)]
}
