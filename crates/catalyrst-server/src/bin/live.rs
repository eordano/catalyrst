#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use tokio::sync::{Mutex, RwLock};
use tokio_util::io::ReaderStream;
use tracing_subscriber::EnvFilter;

use catalyrst_server::routes::build_router;
use catalyrst_server::state::*;

#[derive(Clone)]
struct CachedEntity {
    entity_id: String,
    entity_type: &'static str,
    pointers: Vec<String>,
    bytes: Bytes,
}

struct EntityCache {
    by_id: HashMap<String, CachedEntity>,
    pointer_to_id: HashMap<String, String>,
    by_type: HashMap<&'static str, Vec<String>>,
}

impl EntityCache {
    fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            pointer_to_id: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    fn upsert(&mut self, entity: CachedEntity) {
        if let Some(old) = self.by_id.get(&entity.entity_id) {
            for ptr in &old.pointers {
                if self.pointer_to_id.get(ptr).map(|id| id == &entity.entity_id).unwrap_or(false) {
                    self.pointer_to_id.remove(ptr);
                }
            }
        }

        for ptr in &entity.pointers {
            self.pointer_to_id.insert(ptr.clone(), entity.entity_id.clone());
        }

        let etype = entity.entity_type;
        let eid = entity.entity_id.clone();
        self.by_id.insert(entity.entity_id.clone(), entity);

        let type_vec = self.by_type.entry(etype).or_default();
        if !type_vec.contains(&eid) {
            type_vec.push(eid);
        }
    }

    fn remove(&mut self, entity_id: &str) {
        if let Some(old) = self.by_id.remove(entity_id) {
            for ptr in &old.pointers {
                if self.pointer_to_id.get(ptr).map(|id| id == entity_id).unwrap_or(false) {
                    self.pointer_to_id.remove(ptr);
                }
            }
            if let Some(type_vec) = self.by_type.get_mut(old.entity_type) {
                type_vec.retain(|id| id != entity_id);
            }
        }
    }
}

struct ProfileLru {
    map: HashMap<String, (Instant, Value)>,
    order: VecDeque<String>,
    max_entries: usize,
}

impl ProfileLru {
    fn new(max_entries: usize) -> Self {
        Self {
            map: HashMap::with_capacity(max_entries),
            order: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    fn get(&self, entity_id: &str) -> Option<&Value> {
        self.map.get(entity_id).map(|(_, v)| v)
    }

    fn insert(&mut self, entity_id: String, value: Value) {
        if self.map.contains_key(&entity_id) {
            self.map.insert(entity_id.clone(), (Instant::now(), value));
            self.order.retain(|id| id != &entity_id);
            self.order.push_back(entity_id);
            return;
        }

        while self.map.len() >= self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }

        self.map.insert(entity_id.clone(), (Instant::now(), value));
        self.order.push_back(entity_id);
    }

    fn remove(&mut self, entity_id: &str) {
        if self.map.remove(entity_id).is_some() {
            self.order.retain(|id| id != entity_id);
        }
    }
}

struct PrefixIdsCache {
    map: HashMap<String, (Instant, Arc<Vec<String>>)>,
    order: VecDeque<String>,
    max_entries: usize,
    ttl: std::time::Duration,
}

impl PrefixIdsCache {
    fn new(max_entries: usize, ttl: std::time::Duration) -> Self {
        Self {
            map: HashMap::with_capacity(max_entries),
            order: VecDeque::with_capacity(max_entries),
            max_entries,
            ttl,
        }
    }

    fn get(&self, prefix: &str) -> Option<Arc<Vec<String>>> {
        let (inserted, ids) = self.map.get(prefix)?;
        if inserted.elapsed() >= self.ttl {
            return None;
        }
        Some(ids.clone())
    }

    fn insert(&mut self, prefix: String, ids: Arc<Vec<String>>) {
        if self.map.contains_key(&prefix) {
            self.map.insert(prefix.clone(), (Instant::now(), ids));
            self.order.retain(|p| p != &prefix);
            self.order.push_back(prefix);
            return;
        }

        while self.map.len() >= self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }

        self.map.insert(prefix.clone(), (Instant::now(), ids));
        self.order.push_back(prefix);
    }
}

const CACHED_ENTITY_TYPES: &[&str] = &["scene", "wearable", "emote", "store", "outfits"];

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
}

fn load_env_file(path: &str) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

struct LiveContentStorage {
    inner: catalyrst_storage::ContentStorage,
}

#[async_trait]
impl ContentStorage for LiveContentStorage {
    async fn retrieve(&self, hash: &str) -> Option<Bytes> {
        self.inner.retrieve(hash).await.ok().flatten()
    }

    async fn retrieve_stream(&self, hash: &str) -> Option<(Body, u64)> {
        let (path, _is_gzip) = self.inner.file_path(hash).await.ok()??;
        let file = tokio::fs::File::open(&path).await.ok()?;
        let metadata = file.metadata().await.ok()?;
        let size = metadata.len();
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        Some((body, size))
    }

    async fn retrieve_range(&self, hash: &str, start: u64, end: u64) -> Option<Bytes> {
        let data = self.inner.retrieve_uncompressed(hash).await.ok().flatten()?;
        let start = start as usize;
        let end = (end as usize).min(data.len().saturating_sub(1));
        if start > end || start >= data.len() {
            return None;
        }
        Some(data.slice(start..=end))
    }

    async fn file_info(&self, hash: &str) -> Option<FileInfo> {
        let info = self.inner.file_info(hash).await.ok()??;
        Some(FileInfo {
            size: Some(info.size),
            content_size: info.content_size,
            encoding: info.encoding,
        })
    }

    async fn exist_multiple(&self, hashes: &[String]) -> HashMap<String, bool> {
        let refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
        match self.inner.exist_multiple(&refs).await {
            Ok(results) => results.into_iter().collect(),
            Err(_) => hashes.iter().map(|h| (h.clone(), false)).collect(),
        }
    }
}

struct LiveDatabase {
    pool: PgPool,
    entity_cache: Arc<RwLock<EntityCache>>,
    profile_lru: Arc<Mutex<ProfileLru>>,
    prefix_ids_cache: Arc<Mutex<PrefixIdsCache>>,
}

const NON_CANONICAL_INTERN_CAP: usize = 64;

fn non_canonical_intern_pool() -> &'static dashmap::DashMap<String, &'static str> {
    use std::sync::OnceLock;
    static POOL: OnceLock<dashmap::DashMap<String, &'static str>> = OnceLock::new();
    POOL.get_or_init(dashmap::DashMap::new)
}

fn intern_entity_type(s: &str) -> &'static str {
    match s {
        "profile" => "profile",
        "scene" => "scene",
        "wearable" => "wearable",
        "emote" => "emote",
        "store" => "store",
        "outfits" => "outfits",
        _ => {
            let pool = non_canonical_intern_pool();
            if let Some(existing) = pool.get(s) {
                return *existing;
            }
            if pool.len() >= NON_CANONICAL_INTERN_CAP {
                return "unknown";
            }

            let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
            pool.insert(s.to_string(), leaked);
            leaked
        }
    }
}

#[allow(dead_code)]
#[derive(Serialize)]
struct ContentEntry<'a> {
    key: &'a str,
    hash: &'a str,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct AuditInfoResponse<'a> {
    version: &'a str,
    #[serde(rename = "authChain")]
    auth_chain: &'a Value,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(rename = "overwrittenBy")]
    overwritten_by: &'a Option<String>,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct DeploymentItem<'a> {
    #[serde(rename = "entityType")]
    entity_type: &'a str,
    #[serde(rename = "entityId")]
    entity_id: &'a str,
    #[serde(rename = "entityTimestamp")]
    entity_timestamp: i64,
    pointers: &'a Vec<String>,
    content: Vec<ContentEntry<'a>>,
    #[serde(rename = "deployedBy")]
    deployed_by: &'a str,
    #[serde(rename = "entityVersion")]
    entity_version: &'a str,
    #[serde(rename = "auditInfo")]
    audit_info: AuditInfoResponse<'a>,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct EntityResponse<'a> {
    version: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    entity_type: &'a str,
    timestamp: f64,
    pointers: &'a Vec<String>,
    content: Vec<ContentEntry<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

#[derive(Serialize)]
struct PointerChangeDelta<'a> {
    #[serde(rename = "deploymentId")]
    deployment_id: i64,
    #[serde(rename = "entityType")]
    entity_type: &'a str,
    #[serde(rename = "entityId")]
    entity_id: &'a str,
    pointers: &'a Vec<String>,
    #[serde(rename = "entityTimestamp")]
    entity_timestamp: i64,
    metadata: &'a Value,
    #[serde(rename = "deployerAddress")]
    deployer_address: &'a str,
    version: &'a str,
    #[serde(rename = "authChain")]
    auth_chain: &'a Value,
    #[serde(rename = "localTimestamp")]
    local_timestamp: i64,
}

#[derive(Serialize)]
struct DeploymentFiltersResponse<'a> {
    pointers: &'a Vec<String>,
    #[serde(rename = "entityTypes")]
    entity_types: &'a Vec<String>,
    #[serde(rename = "entityIds")]
    entity_ids: &'a Vec<String>,
    #[serde(rename = "from", skip_serializing_if = "Option::is_none")]
    from: Option<i64>,
    #[serde(rename = "to", skip_serializing_if = "Option::is_none")]
    to: Option<i64>,
    #[serde(rename = "onlyCurrentlyPointed", skip_serializing_if = "Option::is_none")]
    only_currently_pointed: Option<bool>,
    #[serde(rename = "deployedBy", skip_serializing_if = "Vec::is_empty")]
    deployed_by: &'a Vec<String>,
}

#[derive(Serialize)]
struct PointerChangesFiltersResponse<'a> {
    #[serde(rename = "entityTypes")]
    entity_types: &'a Vec<String>,
    #[serde(rename = "from", skip_serializing_if = "Option::is_none")]
    from: Option<i64>,
    #[serde(rename = "to", skip_serializing_if = "Option::is_none")]
    to: Option<i64>,
    #[serde(rename = "includeAuthChain")]
    include_auth_chain: bool,
}

const MAX_HISTORY_LIMIT: i64 = 500;

fn curate_limit(limit: Option<i64>) -> i64 {
    match limit {
        Some(l) if l > 0 && l <= MAX_HISTORY_LIMIT => l,
        _ => MAX_HISTORY_LIMIT,
    }
}

fn curate_offset(offset: Option<i64>) -> i64 {
    match offset {
        Some(o) if o >= 0 => o,
        _ => 0,
    }
}

fn deployment_row_to_entity(row: &DeploymentRow) -> Value {
    let content_arr: Vec<Value> = row
        .content
        .iter()
        .map(|(key, hash)| json!({"file": key, "hash": hash}))
        .collect();

    let mut obj = json!({
        "version": &row.version,
        "id": &row.entity_id,
        "type": row.entity_type,
        "timestamp": row.entity_timestamp as i64,
        "pointers": &row.pointers,
        "content": content_arr,
    });

    if let Some(ref m) = row.metadata {
        obj["metadata"] = m.clone();
    }

    obj
}

#[derive(Debug, sqlx::FromRow)]
struct ActiveEntityRow {
    entity_id: String,
    entity_type: String,
    entity_pointers: Vec<String>,
    entity_metadata: Option<Value>,
    entity_timestamp: f64,
    version: String,
    #[allow(dead_code)]
    id: i32,
    content_json: Value,
}

struct DeploymentRow {
    entity_id: String,
    entity_type: &'static str,
    pointers: Vec<String>,
    metadata: Option<Value>,
    entity_timestamp: f64,
    version: String,
    #[allow(dead_code)]
    deployment_id: i32,
    content: Vec<(String, String)>,
}

fn parse_content_json(v: &Value) -> Vec<(String, String)> {
    match v.as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|entry| {
                let key = entry.get("key")?.as_str()?;
                let hash = entry.get("hash")?.as_str()?;
                Some((key.to_string(), hash.to_string()))
            })
            .collect(),
        None => Vec::new(),
    }
}

fn build_entities_from_rows(rows: Vec<ActiveEntityRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|row| {
            let content = parse_content_json(&row.content_json);
            let metadata = row
                .entity_metadata
                .as_ref()
                .and_then(|m| m.get("v").cloned());
            let dr = DeploymentRow {
                entity_id: row.entity_id,
                entity_type: intern_entity_type(&row.entity_type),
                pointers: row.entity_pointers,
                metadata,
                entity_timestamp: row.entity_timestamp,
                version: row.version,
                deployment_id: row.id,
                content,
            };
            deployment_row_to_entity(&dr)
        })
        .collect()
}

fn row_to_cached_entity(row: ActiveEntityRow) -> CachedEntity {
    let content = parse_content_json(&row.content_json);
    let metadata = row
        .entity_metadata
        .as_ref()
        .and_then(|m| m.get("v").cloned());
    let etype = intern_entity_type(&row.entity_type);
    let pointers_lower: Vec<String> = row.entity_pointers.iter().map(|p| p.to_lowercase()).collect();
    let dr = DeploymentRow {
        entity_id: row.entity_id.clone(),
        entity_type: etype,
        pointers: row.entity_pointers,
        metadata,
        entity_timestamp: row.entity_timestamp,
        version: row.version,
        deployment_id: row.id,
        content,
    };
    let value = deployment_row_to_entity(&dr);
    let bytes = Bytes::from(serde_json::to_vec(&value).unwrap_or_default());
    CachedEntity {
        entity_id: row.entity_id,
        entity_type: etype,
        pointers: pointers_lower,
        bytes,
    }
}

async fn load_entity_type_into_cache(
    pool: &PgPool,
    cache: &mut EntityCache,
    entity_type: &str,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<ActiveEntityRow> = sqlx::query_as(
        r#"
        SELECT
            dep.entity_id,
            dep.entity_type,
            dep.entity_pointers,
            dep.entity_metadata,
            date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
            dep.version,
            dep.id,
            COALESCE(
                (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                 FROM content_files cf WHERE cf.deployment = dep.id),
                '[]'::json
            ) AS content_json
        FROM deployments dep
        INNER JOIN active_pointers ap ON ap.entity_id = dep.entity_id
        WHERE dep.entity_type = $1
          AND dep.deleter_deployment IS NULL
        GROUP BY dep.id
        "#,
    )
    .bind(entity_type)
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    for row in rows {
        let entity = row_to_cached_entity(row);
        cache.upsert(entity);
    }
    Ok(count)
}

async fn refresh_entity_in_cache(
    pool: &PgPool,
    cache: &Arc<RwLock<EntityCache>>,
    entity_type: &str,
    entity_id: &str,
) {
    let row: Option<ActiveEntityRow> = match sqlx::query_as(
        r#"
        SELECT
            dep.entity_id,
            dep.entity_type,
            dep.entity_pointers,
            dep.entity_metadata,
            date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
            dep.version,
            dep.id,
            COALESCE(
                (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                 FROM content_files cf WHERE cf.deployment = dep.id),
                '[]'::json
            ) AS content_json
        FROM deployments dep
        WHERE dep.entity_id = $1
          AND dep.entity_type = $2
          AND dep.deleter_deployment IS NULL
        LIMIT 1
        "#,
    )
    .bind(entity_id)
    .bind(entity_type)
    .fetch_optional(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(entity_id, entity_type, error = %e, "Failed to refresh entity in cache");
            return;
        }
    };

    let mut cache = cache.write().await;
    match row {
        Some(row) => {
            let entity = row_to_cached_entity(row);
            cache.upsert(entity);
            tracing::debug!(entity_id, entity_type, "Cache: refreshed entity");
        }
        None => {
            cache.remove(entity_id);
            tracing::debug!(entity_id, entity_type, "Cache: removed deleted entity");
        }
    }
}

async fn install_notify_trigger(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE OR REPLACE FUNCTION notify_new_deployment() RETURNS trigger AS $$
        BEGIN
            PERFORM pg_notify('new_deployment', NEW.entity_type || ':' || NEW.entity_id);
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DROP TRIGGER IF EXISTS deployment_notify_trigger ON deployments;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER deployment_notify_trigger
            AFTER INSERT ON deployments
            FOR EACH ROW EXECUTE FUNCTION notify_new_deployment();
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn listen_for_invalidations(
    pool: PgPool,
    entity_cache: Arc<RwLock<EntityCache>>,
    profile_lru: Arc<Mutex<ProfileLru>>,
) {
    let mut listener = match sqlx::postgres::PgListener::connect_with(&pool).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create PgListener — cache invalidation disabled");
            return;
        }
    };

    if let Err(e) = listener.listen("new_deployment").await {
        tracing::error!(error = %e, "Failed to LISTEN on new_deployment channel");
        return;
    }

    tracing::info!("Listening for deployment notifications on 'new_deployment' channel");

    loop {
        let notification = match listener.recv().await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "PgListener recv error — reconnecting");
                continue;
            }
        };

        let payload = notification.payload();
        let Some((entity_type, entity_id)) = payload.split_once(':') else {
            tracing::warn!(payload, "Malformed NOTIFY payload — expected 'type:id'");
            continue;
        };

        match entity_type {
            "profile" => {
                let mut lru = profile_lru.lock().await;
                lru.remove(entity_id);
                tracing::debug!(entity_id, "Profile LRU: invalidated");
            }
            _ => {
                if CACHED_ENTITY_TYPES.contains(&entity_type) {
                    refresh_entity_in_cache(&pool, &entity_cache, entity_type, entity_id).await;
                }
            }
        }
    }
}

#[allow(dead_code)]
fn is_cached_type(entity_type: &str) -> bool {
    CACHED_ENTITY_TYPES.contains(&entity_type)
}

#[async_trait]
impl Database for LiveDatabase {
    async fn active_entities_by_pointers(
        &self,
        pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError> {
        if pointers.is_empty() {
            return Ok(vec![]);
        }

        let lower_pointers: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();

        let mut results: Vec<Value> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut uncached_pointers: Vec<String> = Vec::new();

        {
            let cache = self.entity_cache.read().await;
            for ptr in &lower_pointers {
                if let Some(entity_id) = cache.pointer_to_id.get(ptr) {
                    if seen_ids.insert(entity_id.clone()) {
                        if let Some(entity) = cache.by_id.get(entity_id) {

                            if let Ok(val) = serde_json::from_slice::<Value>(&entity.bytes) {
                                results.push(val);
                            }
                        }
                    }
                } else {
                    uncached_pointers.push(ptr.clone());
                }
            }
        }

        if !uncached_pointers.is_empty() {
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id,
                    dep.entity_type,
                    dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version,
                    dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM active_pointers ap
                INNER JOIN deployments dep ON dep.entity_id = ap.entity_id
                WHERE ap.pointer = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(&uncached_pointers)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            for row in rows {
                if seen_ids.insert(row.entity_id.clone()) {
                    let entity_type = row.entity_type.clone();
                    let entity_id = row.entity_id.clone();
                    let entities = build_entities_from_rows(vec![row]);
                    if let Some(value) = entities.into_iter().next() {
                        if entity_type == "profile" {
                            let mut lru = self.profile_lru.lock().await;
                            lru.insert(entity_id, value.clone());
                        }
                        results.push(value);
                    }
                }
            }
        }

        Ok(results)
    }

    async fn active_entities_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, DatabaseError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut results: Vec<Value> = Vec::new();
        let mut uncached_ids: Vec<String> = Vec::new();

        {
            let cache = self.entity_cache.read().await;
            let lru = self.profile_lru.lock().await;
            for id in ids {
                if let Some(entity) = cache.by_id.get(id) {

                    if let Ok(val) = serde_json::from_slice::<Value>(&entity.bytes) {
                        results.push(val);
                    }
                } else if let Some(value) = lru.get(id) {
                    results.push(value.clone());
                } else {
                    uncached_ids.push(id.clone());
                }
            }
        }

        if !uncached_ids.is_empty() {
            let rows: Vec<ActiveEntityRow> = sqlx::query_as(
                r#"
                SELECT
                    dep.entity_id,
                    dep.entity_type,
                    dep.entity_pointers,
                    dep.entity_metadata,
                    date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                    dep.version,
                    dep.id,
                    COALESCE(
                        (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash))
                         FROM content_files cf WHERE cf.deployment = dep.id),
                        '[]'::json
                    ) AS content_json
                FROM deployments dep
                WHERE dep.entity_id = ANY($1)
                  AND dep.deleter_deployment IS NULL
                "#,
            )
            .bind(&uncached_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            for row in rows {
                let entity_type = row.entity_type.clone();
                let entity_id = row.entity_id.clone();
                let entities = build_entities_from_rows(vec![row]);
                if let Some(value) = entities.into_iter().next() {
                    if entity_type == "profile" {
                        let mut lru = self.profile_lru.lock().await;
                        lru.insert(entity_id, value.clone());
                    }
                    results.push(value);
                }
            }
        }

        Ok(results)
    }

    async fn active_entities_by_prefix(
        &self,
        prefix: &str,
        offset: i64,
        limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError> {

        let cached = {
            let cache = self.prefix_ids_cache.lock().await;
            cache.get(prefix)
        };

        let entity_ids: Arc<Vec<String>> = match cached {
            Some(ids) => ids,
            None => {
                let ids = catalyrst_db::pointers_repository::get_item_entities_ids_matching_collection_urn_prefix(
                    &self.pool, prefix,
                )
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                let ids = Arc::new(ids);
                let mut cache = self.prefix_ids_cache.lock().await;
                cache.insert(prefix.to_string(), ids.clone());
                ids
            }
        };

        let total = entity_ids.len() as i64;

        if entity_ids.is_empty() {
            return Ok(PrefixQueryResult {
                total: 0,
                entities: vec![],
            });
        }

        let start = offset as usize;
        let end = ((offset + limit) as usize).min(entity_ids.len());
        if start >= entity_ids.len() {
            return Ok(PrefixQueryResult {
                total,
                entities: vec![],
            });
        }
        let page_ids: Vec<String> = entity_ids[start..end].to_vec();

        let entities = self.active_entities_by_ids(&page_ids).await?;

        Ok(PrefixQueryResult { total, entities })
    }

    async fn active_entity_ids_by_content_hash(
        &self,
        hash: &str,
    ) -> Result<Vec<String>, DatabaseError> {
        catalyrst_db::deployments_repository::get_active_deployments_by_content_hash(
            &self.pool, hash,
        )
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
    }

    async fn get_deployments(
        &self,
        options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError> {
        let offset = curate_offset(options.offset);
        let limit = curate_limit(options.limit);
        let fetch_limit = limit + 1;

        let needs_audit = options.fields.iter().any(|f| f == "auditInfo");

        let sorting_field = options
            .sorting_field
            .as_deref()
            .unwrap_or("local_timestamp");
        let sorting_order = options.sorting_order.as_deref().unwrap_or("DESC");

        let ts_col = match sorting_field {
            "entity_timestamp" => "entity_timestamp",
            _ => "local_timestamp",
        };
        let order = match sorting_order {
            "ASC" => "ASC",
            _ => "DESC",
        };

        let auth_select = if needs_audit {
            "dep1.auth_chain, dep1.deployer_address,"
        } else {
            "NULL::json AS auth_chain, dep1.deployer_address,"
        };

        let mut sql = format!(
            r#"
            SELECT
                dep1.id,
                dep1.entity_type,
                dep1.entity_id,
                dep1.entity_pointers,
                date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
                dep1.entity_metadata,
                {}
                dep1.version,
                date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
                dep1.deleter_deployment
            FROM deployments AS dep1
            "#,
            auth_select,
        );

        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx: usize = 1;

        let from_val = options.from.map(|f| f as f64);
        let to_val = options.to.map(|t| t as f64);
        let last_id = options.last_id.as_deref();

        if let Some(_from) = from_val {
            if ts_col == "local_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "ASC" {
                        conditions.push(format!(
                            "((LOWER(dep1.entity_id) > LOWER(${next}) AND dep1.local_timestamp = to_timestamp(${ts} / 1000.0)) OR (dep1.local_timestamp > to_timestamp(${ts} / 1000.0)))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.local_timestamp >= to_timestamp(${} / 1000.0)", param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.local_timestamp >= to_timestamp(${} / 1000.0)", param_idx
                    ));
                    param_idx += 1;
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "ASC" {
                        conditions.push(format!(
                            "((LOWER(dep1.entity_id) > LOWER(${next}) AND dep1.entity_timestamp = to_timestamp(${ts} / 1000.0)) OR (dep1.entity_timestamp > to_timestamp(${ts} / 1000.0)))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.entity_timestamp >= to_timestamp(${} / 1000.0)", param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.entity_timestamp >= to_timestamp(${} / 1000.0)", param_idx
                    ));
                    param_idx += 1;
                }
            }
        }
        if let Some(_to) = to_val {
            if ts_col == "local_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "DESC" {
                        conditions.push(format!(
                            "((LOWER(dep1.entity_id) < LOWER(${next}) AND dep1.local_timestamp = to_timestamp(${ts} / 1000.0)) OR (dep1.local_timestamp < to_timestamp(${ts} / 1000.0)))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.local_timestamp <= to_timestamp(${} / 1000.0)", param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.local_timestamp <= to_timestamp(${} / 1000.0)", param_idx
                    ));
                    param_idx += 1;
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(_lid) = last_id {
                    if order == "DESC" {
                        conditions.push(format!(
                            "((LOWER(dep1.entity_id) < LOWER(${next}) AND dep1.entity_timestamp = to_timestamp(${ts} / 1000.0)) OR (dep1.entity_timestamp < to_timestamp(${ts} / 1000.0)))",
                            next = param_idx, ts = param_idx + 1,
                        ));
                        param_idx += 2;
                    } else {
                        conditions.push(format!(
                            "dep1.entity_timestamp <= to_timestamp(${} / 1000.0)", param_idx
                        ));
                        param_idx += 1;
                    }
                } else {
                    conditions.push(format!(
                        "dep1.entity_timestamp <= to_timestamp(${} / 1000.0)", param_idx
                    ));
                    param_idx += 1;
                }
            }
        }

        if !options.entity_types.is_empty() {
            conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
            param_idx += 1;
        }
        if !options.entity_ids.is_empty() {
            conditions.push(format!("dep1.entity_id = ANY(${})", param_idx));
            param_idx += 1;
        }
        if options.only_currently_pointed == Some(true) {
            conditions.push("dep1.deleter_deployment IS NULL".into());
        }
        if !options.pointers.is_empty() {
            conditions.push(format!("dep1.entity_pointers && ${}", param_idx));
            param_idx += 1;
        }
        // deployedBy: filter by deployer. Was parsed (and disables the default
        // time window) + echoed in the `filters` response, but never applied to
        // the query, so ?deployedBy=X returned the whole table unfiltered. Uses
        // the lower(deployer_address) index. deployed_by is already lowercased.
        if !options.deployed_by.is_empty() {
            conditions.push(format!("LOWER(dep1.deployer_address) = ANY(${})", param_idx));
            param_idx += 1;
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " ORDER BY dep1.\"{}\" {}, LOWER(dep1.entity_id) {}",
            ts_col, order, order,
        ));
        sql.push_str(&format!(
            " LIMIT ${} OFFSET ${}",
            param_idx, param_idx + 1
        ));

        #[derive(sqlx::FromRow)]
        #[allow(dead_code)]
        struct DepRow {
            id: i32,
            entity_type: String,
            entity_id: String,
            entity_pointers: Vec<String>,
            entity_timestamp: f64,
            entity_metadata: Option<Value>,
            deployer_address: String,
            version: String,
            auth_chain: Option<Value>,
            local_timestamp: f64,
            #[allow(dead_code)]
            deleter_deployment: Option<i32>,
        }

        let mut query = sqlx::query_as::<_, DepRow>(&sql);

        if let Some(from) = from_val {
            if ts_col == "local_timestamp" {
                if let Some(lid) = last_id {
                    if order == "ASC" {
                        query = query.bind(lid.to_string()).bind(from);
                    } else {
                        query = query.bind(from);
                    }
                } else {
                    query = query.bind(from);
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(lid) = last_id {
                    if order == "ASC" {
                        query = query.bind(lid.to_string()).bind(from);
                    } else {
                        query = query.bind(from);
                    }
                } else {
                    query = query.bind(from);
                }
            }
        }
        if let Some(to) = to_val {
            if ts_col == "local_timestamp" {
                if let Some(lid) = last_id {
                    if order == "DESC" {
                        query = query.bind(lid.to_string()).bind(to);
                    } else {
                        query = query.bind(to);
                    }
                } else {
                    query = query.bind(to);
                }
            }
            if ts_col == "entity_timestamp" {
                if let Some(lid) = last_id {
                    if order == "DESC" {
                        query = query.bind(lid.to_string()).bind(to);
                    } else {
                        query = query.bind(to);
                    }
                } else {
                    query = query.bind(to);
                }
            }
        }
        if !options.entity_types.is_empty() {
            query = query.bind(options.entity_types.clone());
        }
        if !options.entity_ids.is_empty() {
            query = query.bind(options.entity_ids.clone());
        }
        if !options.pointers.is_empty() {
            let lower: Vec<String> = options.pointers.iter().map(|p| p.to_lowercase()).collect();
            query = query.bind(lower);
        }
        if !options.deployed_by.is_empty() {
            let lower: Vec<String> = options.deployed_by.iter().map(|a| a.to_lowercase()).collect();
            query = query.bind(lower);
        }

        query = query.bind(fetch_limit).bind(offset);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let more_data = rows.len() as i64 > limit;
        let rows: Vec<DepRow> = if more_data {
            rows.into_iter().take(limit as usize).collect()
        } else {
            rows
        };

        let deployment_ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        let content_map = if deployment_ids.is_empty() {
            HashMap::new()
        } else {
            let cf_rows: Vec<(i32, String, String)> = sqlx::query_as(
                "SELECT deployment, content_hash, key FROM content_files WHERE deployment = ANY($1)"
            )
            .bind(&deployment_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let mut map: HashMap<i32, Vec<(String, String)>> = HashMap::new();
            for (dep_id, hash, key) in cf_rows {
                map.entry(dep_id).or_default().push((key, hash));
            }
            map
        };

        let empty_auth = Value::Array(vec![]);
        let empty_content: Vec<(String, String)> = vec![];

        let deployment_values: Vec<Value> = rows
            .iter()
            .map(|d| {
                let content = content_map.get(&d.id).unwrap_or(&empty_content);
                let metadata = d
                    .entity_metadata
                    .as_ref()
                    .and_then(|m| m.get("v").cloned());

                let auth_chain_ref = d.auth_chain.as_ref().unwrap_or(&empty_auth);
                let interned_type = intern_entity_type(&d.entity_type);

                let content_arr: Vec<Value> = content
                    .iter()
                    .map(|(key, hash)| json!({"key": key, "hash": hash}))
                    .collect();

                let mut obj = json!({
                    "entityType": interned_type,
                    "entityId": &d.entity_id,
                    "entityTimestamp": d.entity_timestamp as i64,
                    "pointers": &d.entity_pointers,
                    "content": content_arr,
                    "deployedBy": &d.deployer_address,
                    "entityVersion": &d.version,
                    "auditInfo": {
                        "version": &d.version,
                        "authChain": auth_chain_ref,
                        "localTimestamp": d.local_timestamp as i64,
                    },
                    "localTimestamp": d.local_timestamp as i64,
                });

                if let Some(ref m) = metadata {
                    obj["metadata"] = m.clone();
                }

                obj
            })
            .collect();

        let filters_json = serde_json::to_value(&DeploymentFiltersResponse {
            pointers: &options.pointers,
            entity_types: &options.entity_types,
            entity_ids: &options.entity_ids,
            from: options.from,
            to: options.to,
            only_currently_pointed: options.only_currently_pointed,
            deployed_by: &options.deployed_by,
        }).unwrap_or_default();

        Ok(DeploymentQueryResult {
            deployments: deployment_values,
            filters: filters_json,
            pagination: PaginationResult {
                offset,
                limit,
                more_data,
                next: None,
                last_id: options.last_id.clone(),
            },
        })
    }

    async fn get_pointer_changes(
        &self,
        options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError> {
        let offset = curate_offset(options.offset);
        let limit = curate_limit(options.limit);
        let fetch_limit = limit + 1;

        let sorting_field = options
            .sorting_field
            .as_deref()
            .unwrap_or("local_timestamp");
        let sorting_order = options.sorting_order.as_deref().unwrap_or("DESC");

        let ts_col = match sorting_field {
            "entity_timestamp" => "entity_timestamp",
            _ => "local_timestamp",
        };
        let order = match sorting_order {
            "ASC" => "ASC",
            _ => "DESC",
        };

        let mut sql = String::from(
            r#"
            SELECT
                dep1.id AS deployment_id,
                dep1.entity_type,
                dep1.entity_id,
                dep1.entity_pointers,
                date_part('epoch', dep1.local_timestamp) * 1000 AS local_timestamp,
                date_part('epoch', dep1.entity_timestamp) * 1000 AS entity_timestamp,
                dep1.deployer_address,
                dep1.version,
                dep1.entity_metadata,
                dep1.auth_chain
            FROM deployments AS dep1
            "#,
        );

        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx: usize = 1;

        if let Some(_from) = options.from {
            conditions.push(format!(
                "dep1.{} >= to_timestamp(${} / 1000.0)",
                ts_col, param_idx
            ));
            param_idx += 1;
        }
        if let Some(_to) = options.to {
            if let Some(_lid) = options.last_id.as_deref() {
                if order == "DESC" {
                    conditions.push(format!(
                        "((LOWER(dep1.entity_id) < LOWER(${next}) AND dep1.{col} = to_timestamp(${ts} / 1000.0)) OR (dep1.{col} < to_timestamp(${ts} / 1000.0)))",
                        next = param_idx, ts = param_idx + 1, col = ts_col,
                    ));
                    param_idx += 2;
                } else {
                    conditions.push(format!(
                        "dep1.{} <= to_timestamp(${} / 1000.0)",
                        ts_col, param_idx
                    ));
                    param_idx += 1;
                }
            } else {
                conditions.push(format!(
                    "dep1.{} <= to_timestamp(${} / 1000.0)",
                    ts_col, param_idx
                ));
                param_idx += 1;
            }
        }

        if !options.entity_types.is_empty() {
            conditions.push(format!("dep1.entity_type = ANY(${})", param_idx));
            param_idx += 1;
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " ORDER BY dep1.\"{}\" {}, LOWER(dep1.entity_id) {}",
            ts_col, order, order,
        ));

        sql.push_str(&format!(
            " LIMIT ${} OFFSET ${}",
            param_idx,
            param_idx + 1
        ));

        #[derive(sqlx::FromRow)]
        struct PointerChangeRow {
            deployment_id: i32,
            entity_type: String,
            entity_id: String,
            entity_pointers: Vec<String>,
            local_timestamp: f64,
            entity_timestamp: f64,
            deployer_address: String,
            version: String,
            entity_metadata: Option<Value>,
            auth_chain: Value,
        }

        let mut query = sqlx::query_as::<_, PointerChangeRow>(&sql);

        if let Some(from) = options.from {
            query = query.bind(from as f64);
        }
        if let Some(to) = options.to {
            if let Some(lid) = options.last_id.as_deref() {
                if order == "DESC" {
                    query = query.bind(lid.to_string()).bind(to as f64);
                } else {
                    query = query.bind(to as f64);
                }
            } else {
                query = query.bind(to as f64);
            }
        }
        if !options.entity_types.is_empty() {
            query = query.bind(&options.entity_types);
        }

        query = query.bind(fetch_limit).bind(offset);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let more_data = rows.len() as i64 > limit;
        let rows: Vec<PointerChangeRow> = if more_data {
            rows.into_iter().take(limit as usize).collect()
        } else {
            rows
        };

        const NULL_METADATA: Value = Value::Null;
        let deltas: Vec<Value> = rows
            .iter()
            .map(|r| {
                let delta = PointerChangeDelta {
                    deployment_id: r.deployment_id as i64,
                    entity_type: intern_entity_type(&r.entity_type),
                    entity_id: &r.entity_id,
                    pointers: &r.entity_pointers,
                    entity_timestamp: r.entity_timestamp as i64,
                    metadata: r
                        .entity_metadata
                        .as_ref()
                        .and_then(|m| m.get("v"))
                        .unwrap_or(&NULL_METADATA),
                    deployer_address: &r.deployer_address,
                    version: &r.version,
                    auth_chain: &r.auth_chain,
                    local_timestamp: r.local_timestamp as i64,
                };
                serde_json::to_value(&delta).unwrap_or_default()
            })
            .collect();

        let filters_json = serde_json::to_value(&PointerChangesFiltersResponse {
            entity_types: &options.entity_types,
            from: options.from,
            to: options.to,
            include_auth_chain: options.include_auth_chain,
        }).unwrap_or_default();

        Ok(PointerChangesQueryResult {
            deltas,
            filters: filters_json,
            pagination: PaginationResult {
                offset,
                limit,
                more_data,
                next: None,
                last_id: options.last_id.clone(),
            },
        })
    }

    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
        #[derive(Serialize)]
        struct FailedDeploymentResponse {
            #[serde(rename = "entityId")]
            entity_id: String,
            #[serde(rename = "entityType")]
            entity_type: String,
            #[serde(rename = "failureTimestamp")]
            failure_timestamp: i64,
            reason: String,
            #[serde(rename = "authChain")]
            auth_chain: Value,
            #[serde(rename = "errorDescription")]
            error_description: String,
            #[serde(rename = "snapshotHash")]
            snapshot_hash: String,
        }

        let rows =
            catalyrst_db::failed_deployments_repository::get_snapshot_failed_deployments(&self.pool)
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|fd| {
                serde_json::to_value(&FailedDeploymentResponse {
                    entity_id: fd.entity_id,
                    entity_type: fd.entity_type,
                    failure_timestamp: fd.failure_timestamp as i64,
                    reason: fd.reason,
                    auth_chain: fd.auth_chain,
                    error_description: fd.error_description,
                    snapshot_hash: fd.snapshot_hash,
                }).unwrap_or_default()
            })
            .collect())
    }

    async fn get_audit_info(
        &self,
        _entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError> {
        #[derive(sqlx::FromRow)]
        struct AuditRow {
            version: String,
            auth_chain: Value,
            local_timestamp: f64,
        }

        let row: Option<AuditRow> = sqlx::query_as(
            r#"
            SELECT
                version,
                auth_chain,
                date_part('epoch', local_timestamp) * 1000 AS local_timestamp
            FROM deployments
            WHERE entity_id = $1
            LIMIT 1
            "#,
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        #[derive(Serialize)]
        struct AuditInfoDetail {
            version: String,
            #[serde(rename = "authChain")]
            auth_chain: Value,
            #[serde(rename = "localTimestamp")]
            local_timestamp: i64,
        }

        Ok(row.map(|r| {
            serde_json::to_value(&AuditInfoDetail {
                version: r.version,
                auth_chain: r.auth_chain,
                local_timestamp: r.local_timestamp as i64,
            }).unwrap_or_default()
        }))
    }

    async fn find_entity_by_pointer(&self, pointer: &str) -> Result<Option<Value>, DatabaseError> {
        let lower = pointer.to_lowercase();
        let pointers = vec![lower];
        let mut entities = self.active_entities_by_pointers(&pointers).await?;
        Ok(entities.pop())
    }
}

struct ReadOnlyDeployer;

#[async_trait]
impl Deployer for ReadOnlyDeployer {
    async fn deploy_entity(
        &self,
        _files: Vec<Bytes>,
        _entity_id: &str,
        _auth_chain: Value,
        _context: &str,
    ) -> Result<i64, Vec<String>> {
        Err(vec![
            "Live server is read-only; deployments are not supported".to_string(),
        ])
    }
}

struct EmptyDenylist;
impl Denylist for EmptyDenylist {
    fn is_denylisted(&self, _id: &str) -> bool {
        false
    }
}

struct UuidChallengeSupervisor;
impl ChallengeSupervisor for UuidChallengeSupervisor {
    fn get_challenge_text(&self) -> String {
        format!("dcl-crypto-{}", uuid::Uuid::new_v4())
    }
}

struct LiveSynchronizationState {
    sync_state: Option<Arc<tokio::sync::RwLock<catalyrst_sync::SyncState>>>,
}

impl LiveSynchronizationState {
    fn new() -> Self {
        Self { sync_state: None }
    }

    fn with_sync_state(sync_state: Arc<tokio::sync::RwLock<catalyrst_sync::SyncState>>) -> Self {
        Self { sync_state: Some(sync_state) }
    }

    fn read_state(&self) -> Option<catalyrst_sync::SyncState> {
        let handle = self.sync_state.as_ref()?;
        Some(handle.try_read().ok()?.clone())
    }
}

impl SynchronizationState for LiveSynchronizationState {
    fn get_state(&self) -> String {
        match self.read_state() {
            None => "Syncing".to_string(),
            Some(catalyrst_sync::SyncState::Bootstrapping) => "Bootstrapping".to_string(),
            Some(catalyrst_sync::SyncState::PartiallySynced { .. }) => "Syncing".to_string(),
            Some(catalyrst_sync::SyncState::Syncing) => "Syncing".to_string(),
        }
    }

    fn is_type_ready(&self, entity_type: &str) -> bool {
        match self.read_state() {
            None => true,
            Some(catalyrst_sync::SyncState::Syncing) => true,
            Some(catalyrst_sync::SyncState::PartiallySynced { ready_types }) => {
                ready_types.contains(entity_type)
            }
            Some(catalyrst_sync::SyncState::Bootstrapping) => false,
        }
    }

    fn ready_types(&self) -> Option<Vec<String>> {
        match self.read_state() {
            None => None,
            Some(catalyrst_sync::SyncState::Syncing) => None,
            Some(catalyrst_sync::SyncState::PartiallySynced { ready_types }) => {
                let mut types: Vec<String> = ready_types.iter().cloned().collect();
                types.sort();
                Some(types)
            }
            Some(catalyrst_sync::SyncState::Bootstrapping) => Some(vec![]),
        }
    }
}

struct LiveSnapshotGenerator {
    snapshots: Arc<RwLock<Option<Value>>>,
}

impl LiveSnapshotGenerator {
    async fn load(pool: &PgPool) -> Self {
        #[derive(sqlx::FromRow)]
        struct SnapRow {
            hash: Option<String>,
            init_ts_ms: f64,
            end_ts_ms: f64,
            number_of_entities: i32,
            replaced_hashes: Vec<String>,
            gen_ts_ms: f64,
        }

        let rows = sqlx::query_as::<_, SnapRow>(
            r#"
            SELECT hash,
                   (EXTRACT(EPOCH FROM init_timestamp) * 1000)::float8 AS init_ts_ms,
                   (EXTRACT(EPOCH FROM end_timestamp) * 1000)::float8 AS end_ts_ms,
                   number_of_entities,
                   replaced_hashes,
                   (EXTRACT(EPOCH FROM generation_time) * 1000)::float8 AS gen_ts_ms
            FROM snapshots
            ORDER BY end_timestamp DESC
            "#,
        )
        .fetch_all(pool)
        .await;

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "Failed to load snapshots");
                return Self { snapshots: Arc::new(RwLock::new(None)) };
            }
        };

        if rows.is_empty() {
            tracing::warn!("No snapshots found in database");
            return Self { snapshots: Arc::new(RwLock::new(None)) };
        }

        let arr: Vec<Value> = rows.iter().map(|r| {
            serde_json::json!({
                "hash": r.hash,
                "timeRange": {
                    "initTimestamp": r.init_ts_ms as i64,
                    "endTimestamp": r.end_ts_ms as i64,
                },
                "replacedSnapshotHashes": r.replaced_hashes,
                "numberOfEntities": r.number_of_entities,
                "generationTimestamp": r.gen_ts_ms as i64,
            })
        }).collect();

        tracing::info!(count = arr.len(), "Snapshots loaded into memory");
        Self { snapshots: Arc::new(RwLock::new(Some(Value::Array(arr)))) }
    }

    fn snapshots_handle(&self) -> Arc<RwLock<Option<Value>>> {
        self.snapshots.clone()
    }
}

fn snapshots_metadata_to_json(snapshots: &[catalyrst_db::snapshots_repository::SnapshotMetadata]) -> Value {
    let arr: Vec<Value> = snapshots.iter().map(|s| {
        json!({
            "hash": s.hash,
            "timeRange": {
                "initTimestamp": s.time_range.init_timestamp as i64,
                "endTimestamp": s.time_range.end_timestamp as i64,
            },
            "replacedSnapshotHashes": s.replaced_snapshot_hashes,
            "numberOfEntities": s.number_of_entities,
            "generationTimestamp": s.generation_timestamp as i64,
        })
    }).collect();
    Value::Array(arr)
}

impl SnapshotGenerator for LiveSnapshotGenerator {
    fn get_current_snapshots(&self) -> Option<Value> {
        self.snapshots.try_read().ok()?.clone()
    }
}

struct LiveContentCluster;
#[async_trait]
impl ContentCluster for LiveContentCluster {
    fn get_status(&self) -> Value {
        json!({})
    }
}

#[tokio::main]
async fn main() {

    load_env_file("/etc/catalyrst/content.env");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    catalyrst_server::metrics::init();

    let port: u16 = env_or("CATALYRST_PORT", "5141")
        .parse()
        .expect("CATALYRST_PORT must be a valid port number");
    let host = env_or("HTTP_SERVER_HOST", "127.0.0.1");
    let content_version = env_or("CONTENT_VERSION", "7.6.1+rust");
    let lambdas_version = env_or("LAMBDAS_VERSION", "4.12.0+rust");
    let commit_hash = env_or("COMMIT_HASH", "unknown");
    let eth_network = env_or("ETH_NETWORK", "mainnet");
    let public_url = env_or("PUBLIC_URL", &format!("http://{}:{}", host, port))
        .trim_end_matches('/')
        .to_string();
    let content_server_address = env_or(
        "CONTENT_SERVER_ADDRESS",
        &format!("{}/content", public_url),
    );

    let pg_host = env_or("POSTGRES_HOST", "/run/postgresql");
    let pg_port = env_or("POSTGRES_PORT", "5432");
    let pg_user = env_or("POSTGRES_CONTENT_USER", "");
    if pg_user.is_empty() {
        panic!("POSTGRES_CONTENT_USER must be set (env var or /etc/catalyrst/content.env)");
    }
    let pg_password = env_or("POSTGRES_CONTENT_PASSWORD", "");
    if pg_password.is_empty() {
        panic!("POSTGRES_CONTENT_PASSWORD must be set (env var or /etc/catalyrst/content.env)");
    }
    let pg_db = env_or("POSTGRES_CONTENT_DB", "content");

    let db_url = if pg_host.starts_with('/') {
        format!(
            "postgres://{}:{}@localhost:{}/{}?host={}",
            pg_user, pg_password, pg_port, pg_db, pg_host
        )
    } else {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            pg_user, pg_password, pg_host, pg_port, pg_db
        )
    };

    tracing::info!(
        db = %pg_db,
        host = %pg_host,
        port = %pg_port,
        "Connecting to postgres"
    );

    let connect_options: PgConnectOptions = db_url
        .parse::<PgConnectOptions>()
        .expect("Failed to parse database URL")

        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);

    let pool = PgPoolOptions::new()
        .max_connections(50)
        .min_connections(10)
        .idle_timeout(std::time::Duration::from_secs(600))
        .max_lifetime(std::time::Duration::from_secs(3600))
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to postgres");

    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .expect("Database connectivity check failed");
    tracing::info!("Database connection verified");

    tracing::info!("Pre-warming prepared statement cache");
    let _ = sqlx::query("SELECT 1 FROM deployments WHERE entity_type = $1 LIMIT 0")
        .bind("profile")
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM content_files WHERE deployment = ANY($1::int[]) LIMIT 0")
        .bind(&[0i32][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM active_pointers WHERE pointer = ANY($1::text[]) LIMIT 0")
        .bind(&[""][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "SELECT 1 FROM deployments WHERE entity_id = ANY($1::text[]) AND deleter_deployment IS NULL LIMIT 0"
    )
        .bind(&[""][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "SELECT 1 FROM deployments WHERE entity_id = $1 LIMIT 0"
    )
        .bind("")
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "SELECT 1 FROM failed_deployments LIMIT 0"
    )
        .execute(&pool)
        .await;
    tracing::info!("Prepared statement cache warmed");

    let storage_root = env_or(
        "STORAGE_ROOT_FOLDER",
        "/var/lib/catalyrst/content",
    );
    tracing::info!(root = %storage_root, "Initializing content storage");

    let content_storage = catalyrst_storage::ContentStorage::new(&storage_root)
        .await
        .expect("Failed to initialize content storage");

    let entity_cache = Arc::new(RwLock::new(EntityCache::new()));
    let profile_lru = Arc::new(Mutex::new(ProfileLru::new(10_000)));

    let prefix_ids_cache = Arc::new(Mutex::new(PrefixIdsCache::new(
        2_000,
        std::time::Duration::from_secs(24 * 60 * 60),
    )));

    let sync_enabled = std::env::var("SYNC_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if !sync_enabled {
        tracing::info!("Loading non-profile entities into memory cache...");
        for entity_type in &["scene", "wearable", "emote", "store", "outfits"] {
            let mut ec = entity_cache.write().await;
            if let Err(e) = load_entity_type_into_cache(&pool, &mut ec, entity_type).await {
                tracing::warn!(entity_type = %entity_type, error = %e, "Failed to load entity type into cache");
            }
        }
        {
            let ec = entity_cache.read().await;
            tracing::info!(
                total = ec.by_id.len(),
                pointers = ec.pointer_to_id.len(),
                "Entity cache loaded"
            );
        }

        let _ = install_notify_trigger(&pool).await;
        tokio::spawn(listen_for_invalidations(
            pool.clone(),
            entity_cache.clone(),
            profile_lru.clone(),
        ));
    } else {
        tracing::info!("Sync mode — skipping entity cache load and NOTIFY listener");
    }

    let content_public_url = env_or("CONTENT_URL", &format!("{}/content/", public_url));
    let lambdas_public_url = env_or("LAMBDAS_URL", &format!("{}/lambdas/", public_url));
    let realm_name = std::env::var("REALM_NAME").ok();

    let profile_cdn_base_url = env_or(
        "PROFILE_CDN_BASE_URL",
        "https://profile-images.decentraland.org",
    );
    let land_image_base_url = env_or("LAND_IMAGE_BASE_URL", "http://127.0.0.1:5143");

    let squid_pool = {
        let squid_host = env_or("SQUID_DB_HOST", &pg_host);
        let squid_port: u16 = env_or("SQUID_DB_PORT", &pg_port).parse().unwrap_or(6432);
        let squid_user = env_or("SQUID_DB_USER", "squid_ro");
        let squid_password = std::env::var("SQUID_DB_PASSWORD").ok();
        let squid_db = env_or("SQUID_DB_NAME", "marketplace_squid");

        let squid_opts = sqlx::postgres::PgConnectOptions::new()
            .host(&squid_host)
            .port(squid_port)
            .username(&squid_user)
            .database(&squid_db)

            .options([
                ("statement_timeout", "60000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ]);

        let squid_opts = match squid_password {
            Some(ref pw) => squid_opts.password(pw),
            None => squid_opts,
        };

        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect_with(squid_opts)
            .await
        {
            Ok(p) => {
                tracing::info!(db = %squid_db, "Connected to squid database for ownership validation");
                Some(p)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Could not connect to squid database — ownership validation disabled (all items pass through)"
                );
                None
            }
        }
    };

    let sync_orchestrator = if sync_enabled {

        let sync_source = env_or("SYNC_SOURCE", "http://127.0.0.1:5140");
        tracing::info!(source = %sync_source, "Preparing sync orchestrator");

        for raw in sync_source.split(',') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            match reqwest::Url::parse(trimmed) {
                Ok(u) => {
                    if u.scheme() == "http" {
                        let host = u.host_str().unwrap_or("");
                        let is_loopback = host == "localhost"
                            || host == "127.0.0.1"
                            || host == "[::1]"
                            || host == "::1";
                        if !is_loopback {
                            panic!(
                                "SYNC_SOURCE entry '{trimmed}' uses plaintext http:// to a \
                                 non-loopback host ({host}); sync ingests arbitrary entities \
                                 from this peer, so a MITM could poison the index. Use \
                                 https:// or set the peer to 127.0.0.1/localhost for dev."
                            );
                        }
                    }
                }
                Err(e) => {
                    panic!("SYNC_SOURCE entry '{trimmed}' is not a valid URL: {e}");
                }
            }
        }

        let sync_storage_root = env_or("SYNC_STORAGE_ROOT", "/var/lib/catalyrst/content_rust");
        let sync_storage = std::sync::Arc::new(
            catalyrst_server::sync_backends::LiveSyncStorage::new(
                catalyrst_storage::ContentStorage::new(&sync_storage_root)
                    .await
                    .expect("Failed to create sync content storage"),
            ),
        );

        let sync_db_name = env_or("SYNC_DB_NAME", "content_rust");
        let sync_pg_user = env_or("POSTGRES_CONTENT_USER", "");
        if sync_pg_user.is_empty() {
            panic!("POSTGRES_CONTENT_USER must be set for sync pool (env var or /etc/catalyrst/content.env)");
        }
        let sync_pg_password = env_or("POSTGRES_CONTENT_PASSWORD", "");
        if sync_pg_password.is_empty() {
            panic!("POSTGRES_CONTENT_PASSWORD must be set for sync pool (env var or /etc/catalyrst/content.env)");
        }
        let sync_opts = sqlx::postgres::PgConnectOptions::new()
            .host(&env_or("POSTGRES_HOST", "/run/postgresql"))
            .port(env_or("POSTGRES_PORT", "5432").parse().unwrap_or(5432))
            .username(&sync_pg_user)
            .password(&sync_pg_password)
            .database(&sync_db_name)
            .options([
                ("statement_timeout", "60000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ]);
        tracing::info!(db = %sync_db_name, "Connecting to sync database");
        let sync_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(40)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect_with(sync_opts)
            .await
            .expect("Failed to connect to sync database");
        tracing::info!("Sync database connected");

        let sync_deployer: std::sync::Arc<dyn catalyrst_sync::Deployer> =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveSyncDeployer::new(sync_pool.clone()));
        let sync_deploy_repo: std::sync::Arc<dyn catalyrst_sync::DeploymentRepository> =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveDeploymentRepository::new(sync_pool.clone()));
        let sync_failed: std::sync::Arc<dyn catalyrst_sync::FailedDeploymentsStore> =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveFailedDeploymentsStore::new(sync_pool.clone()));
        let sync_processed: std::sync::Arc<dyn catalyrst_sync::ProcessedSnapshotStore> =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveProcessedSnapshotStore::new(sync_pool.clone()));

        let snapshot_storage_path = format!("{}/snapshots", sync_storage_root);
        tokio::fs::create_dir_all(&snapshot_storage_path).await.ok();
        let sync_snapshot_check: std::sync::Arc<dyn catalyrst_sync::SnapshotStorageCheck> =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveSnapshotStorageCheck::new(
                catalyrst_storage::SnapshotStorage::new(&snapshot_storage_path)
                    .await
                    .expect("Failed to create snapshot storage"),
            ));

        let content_download_concurrency: usize = env_or("CONCURRENT_SYNC_DOWNLOADS", "200")
            .parse()
            .expect("CONCURRENT_SYNC_DOWNLOADS must be a number");

        let connections_max_idle: usize = env_or("CONNECTIONS_MAX_IDLE", "25")
            .parse()
            .expect("CONNECTIONS_MAX_IDLE must be a number");

        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(connections_max_idle)
            .tcp_nodelay(true)
            .connect_timeout(std::time::Duration::from_secs(8))
            .read_timeout(std::time::Duration::from_secs(25))
            // No total request timeout: this client streams large bodies
            // (snapshot files reach 100s of MB, content blobs too). A total cap
            // truncates slow large downloads → hash-verification failures and
            // skipped snapshots. connect_timeout + read_timeout (idle detection)
            // still catch hung/stalled connections. Callers that want a hard cap
            // (e.g. the small /snapshots index fetch) set a per-request timeout.
            .redirect(reqwest::redirect::Policy::limited(2))
            .build()
            .expect("Failed to create HTTP client");

        let sync_deploy_repo_live = catalyrst_server::sync_backends::LiveDeploymentRepository::new(sync_pool.clone());
        let mut bloom = catalyrst_sync::BloomFilter::new();
        tracing::info!("Loading entity IDs into bloom filter...");
        match sync_deploy_repo_live.load_all_entity_ids().await {
            Ok(ids) => {
                let count = ids.len();
                for id in &ids {
                    bloom.add(id);
                }
                tracing::info!(count, "Bloom filter populated");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load entity IDs for bloom filter, starting empty");
            }
        }

        let batch_deployer = std::sync::Arc::new(catalyrst_sync::batch_deployer::BatchDeployer::with_bloom(
            catalyrst_sync::batch_deployer::BatchDeployerConfig {
                content_download_concurrency,
                ..Default::default()
            },
            http_client.clone(),
            sync_storage.clone(),
            sync_deployer.clone(),
            sync_deploy_repo.clone(),
            sync_failed.clone(),
            bloom,
        ));

        let retry_peers: Vec<String> = sync_source
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let retry_worker = catalyrst_sync::retry_failed::RetryFailedDeployments::new(
            catalyrst_sync::retry_failed::RetryFailedConfig::default(),
            http_client.clone(),
            sync_storage.clone(),
            sync_deployer.clone(),
            sync_failed.clone(),
            std::sync::Arc::new(tokio::sync::RwLock::new(retry_peers)),
        );
        let retry_pool = sync_pool.clone();

        let phased_sync = std::env::var("PHASED_SYNC")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        let orchestrator = catalyrst_sync::sync_orchestrator::SyncOrchestrator::new(
            catalyrst_sync::sync_orchestrator::SyncOrchestratorConfig {
                from_timestamp: 0,
                request_max_retries: 10,
                request_retry_wait_ms: 5000,
                delete_snapshots_after_use: false,
                pointer_changes_wait_time_ms: 10_000,
                bootstrap_reconnect_time_ms: 5000,
                bootstrap_reconnect_exponent: 1.5,
                bootstrap_max_reconnect_ms: 3_600_000,
                syncing_reconnect_time_ms: 1000,
                syncing_reconnect_exponent: 1.5,
                syncing_max_reconnect_ms: 86_400_000,
                re_snapshot_interval_ms: 86_400_000 * 14,
                phased_sync,
            },
            http_client,
            sync_storage,
            batch_deployer,
            sync_processed,
            sync_snapshot_check,
            sync_deploy_repo,
        );

        tracing::info!(phased_sync, "Sync orchestrator ready");
        Some((orchestrator, sync_source, retry_worker, retry_pool))
    } else {
        None
    };

    let sync_state: Arc<dyn SynchronizationState> = match &sync_orchestrator {
        Some((orch, _, _, _)) => Arc::new(LiveSynchronizationState::with_sync_state(orch.state_handle())),
        None => Arc::new(LiveSynchronizationState::new()),
    };

    let snapshot_gen = LiveSnapshotGenerator::load(&pool).await;
    let snapshot_handle = snapshot_gen.snapshots_handle();

    let snapshot_generation_interval_hours: u64 = env_or("SNAPSHOT_GENERATION_INTERVAL_HOURS", "6")
        .parse()
        .expect("SNAPSHOT_GENERATION_INTERVAL_HOURS must be a number");

    let snapshot_storage_path = format!("{}/snapshots", storage_root);
    tokio::fs::create_dir_all(&snapshot_storage_path).await.ok();

    let is_mainnet = eth_network == "mainnet";
    let tpr_subgraph_url = std::env::var("THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL")
        .ok()
        .or_else(|| {
            is_mainnet.then(|| "https://subgraph.decentraland.org/tpr-matic-mainnet".to_string())
        });
    let blocks_l2_subgraph_url = std::env::var("BLOCKS_L2_SUBGRAPH_URL").ok().or_else(|| {
        is_mainnet.then(|| "https://subgraph.decentraland.org/blocks-matic-mainnet".to_string())
    });

    if let (Some(hours), Some(sp), Some(tpr)) = (
        std::env::var("THIRD_PARTY_REFRESH_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|h| *h > 0),
        squid_pool.clone(),
        tpr_subgraph_url.clone(),
    ) {
        let tp = catalyrst_validator::tp_subgraph::TpSubgraph::new(
            blocks_l2_subgraph_url.clone().unwrap_or_default(),
            tpr,
        );
        tracing::info!(hours, "third-party root refresh enabled");
        catalyrst_server::third_party_refresh::spawn(
            sp,
            tp,
            std::time::Duration::from_secs(hours * 3600),
        );
    }

    let enable_deployments = env_or("ENABLE_DEPLOYMENTS", "false") == "true";
    let deployer: Arc<dyn Deployer> = if enable_deployments {
        let ignore_blockchain_access =
            env_or("IGNORE_BLOCKCHAIN_ACCESS_CHECKS", "false") == "true";
        let eth_rpc_url = env_or("ETH_RPC_URL", "https://rpc.decentraland.org/mainnet");
        if eth_rpc_url.starts_with("http://") {
            panic!(
                "ENABLE_DEPLOYMENTS=true but ETH_RPC_URL is plaintext http:// \
                 ({eth_rpc_url}); EIP-1654 signature validation requires a \
                 trusted TLS (https://) endpoint. Refusing to start."
            );
        }
        let additional_dcl_address = std::env::var("ADDITIONAL_DECENTRALAND_ADDRESS").ok();
        let third_party_root_via_squid =
            env_or("THIRD_PARTY_ROOT_SOURCE", "subgraph") == "squid";
        match squid_pool.clone() {
            Some(sp) => {
                let write_storage = catalyrst_storage::ContentStorage::new(&storage_root)
                    .await
                    .expect("failed to init content storage for write deployer");
                tracing::warn!(
                    ignore_blockchain_access,
                    "ENABLE_DEPLOYMENTS=true — serving authoritative writes on POST /entities"
                );
                Arc::new(catalyrst_server::write_deployer::WriteDeployer::new(
                    pool.clone(),
                    Arc::new(write_storage),
                    sp,
                    eth_rpc_url,
                    ignore_blockchain_access,
                    additional_dcl_address,
                    tpr_subgraph_url,
                    blocks_l2_subgraph_url,
                    third_party_root_via_squid,
                )) as Arc<dyn Deployer>
            }
            None => {
                tracing::error!(
                    "ENABLE_DEPLOYMENTS=true but no squid pool is available; \
                     refusing to enable writes (staying read-only)"
                );
                Arc::new(ReadOnlyDeployer) as Arc<dyn Deployer>
            }
        }
    } else {
        Arc::new(ReadOnlyDeployer) as Arc<dyn Deployer>
    };

    let state = Arc::new(AppState {
        storage: Arc::new(LiveContentStorage {
            inner: content_storage,
        }),
        database: Arc::new(LiveDatabase {
            pool: pool.clone(),
            entity_cache: entity_cache.clone(),
            profile_lru: profile_lru.clone(),
            prefix_ids_cache: prefix_ids_cache.clone(),
        }),
        deployer,
        denylist: Arc::new(EmptyDenylist),
        challenge_supervisor: Arc::new(UuidChallengeSupervisor),
        synchronization_state: sync_state.clone(),
        snapshot_generator: Arc::new(snapshot_gen),
        content_cluster: Arc::new(LiveContentCluster),
        deployments_cache: dashmap::DashMap::new(),
        content_version,
        lambdas_version,
        commit_hash,
        eth_network,
        content_server_address,
        read_only: env_bool("READ_ONLY", false),
        entities_cache_control_max_age: env_or("ENTITIES_CACHE_CONTROL_MAX_AGE", "10")
            .parse()
            .unwrap_or(10),
        content_public_url,
        lambdas_public_url,
        realm_name,
        squid_pool,
        profile_cdn_base_url,
        land_image_base_url,
    });

    let app = build_router(state);

    if let Some((orchestrator, sync_source, retry_worker, retry_pool)) = sync_orchestrator {
        let peers: std::collections::HashSet<String> = sync_source
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        tokio::spawn(async move {
            tracing::info!(peers = ?peers, "Sync orchestrator starting...");
            match orchestrator.sync_with_servers(peers).await {
                Ok(handle) => {
                    tracing::info!("Sync started, waiting for bootstrap...");
                    handle.wait_for_bootstrap().await;
                    tracing::info!("Bootstrap complete, sync is now in steady state");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Sync orchestrator failed to start");
                }
            }
        });

        if std::env::var("RETRY_FAILED_ENABLED").map(|v| v != "false" && v != "0").unwrap_or(true) {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                retry_worker.run().await;
            });
        }

        let prune_ttl_days: i64 = env_or("RETRY_FAILED_PRUNE_TTL_DAYS", "7")
            .parse()
            .unwrap_or(7);
        tokio::spawn(async move {
            let day = std::time::Duration::from_secs(86400);
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            loop {
                match sqlx::query("DELETE FROM failed_deployments WHERE failure_time < NOW() - ($1 || ' days')::interval")
                    .bind(prune_ttl_days.to_string())
                    .execute(&retry_pool)
                    .await
                {
                    Ok(r) if r.rows_affected() > 0 => {
                        tracing::info!(pruned = r.rows_affected(), ttl_days = prune_ttl_days, "Pruned old failed_deployments");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to prune old failed_deployments");
                    }
                }
                tokio::time::sleep(day).await;
            }
        });
    }

    {
        let pool = pool.clone();
        let sync_state = sync_state.clone();
        let snapshot_handle = snapshot_handle.clone();
        let storage_root_snap = storage_root.clone();
        let interval = std::time::Duration::from_secs(snapshot_generation_interval_hours * 3600);
        tokio::spawn(async move {
            let content_storage = match catalyrst_storage::ContentStorage::new(&storage_root_snap).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to initialize content storage for snapshot generation");
                    return;
                }
            };

            loop {
                let state_str = sync_state.get_state();
                if state_str == "Syncing" {
                    tracing::info!("Sync state is Syncing, generating time-windowed snapshots...");
                    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
                    match catalyrst_db::snapshot_generator::generate_snapshots_multi(
                        &pool,
                        &content_storage,
                        catalyrst_db::snapshot_generator::SNAPSHOTS_INIT_TIMESTAMP_MS,
                        now_ms,
                    ).await {
                        Ok(metadatas) => {
                            let snap_json = snapshots_metadata_to_json(&metadatas);
                            let mut handle = snapshot_handle.write().await;
                            *handle = Some(snap_json);
                            tracing::info!(count = metadatas.len(), "Snapshot generation complete, endpoint updated");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Snapshot generation failed");
                        }
                    }
                } else {
                    tracing::info!(state = %state_str, "Waiting for Syncing state before generating snapshots");
                }

                tokio::time::sleep(interval).await;
            }
        });
    }

    let bind_addr = format!("{}:{}", host, port);
    tracing::info!(addr = %bind_addr, "catalyrst-live listening");

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind TCP listener");

    // Trim trailing slashes BEFORE routing (the layer must wrap the router, not
    // sit inside it): the explorer POSTs `/content/entities/` (upstream
    // EntitiesDeployment form) which otherwise 404s against the `/entities`
    // route. Same fix as the explore bundle.
    use tower::Layer as _;
    let app = tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash().layer(app);
    axum::serve(
        listener,
        axum::ServiceExt::<axum::extract::Request>::into_make_service(app),
    )
    .await
    .expect("Server error");
}
