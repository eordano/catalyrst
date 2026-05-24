use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

fn checked_f64_to_i64(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    if v < i64::MIN as f64 || v > i64::MAX as f64 {
        return None;
    }
    Some(v as i64)
}

pub struct EntityCache {
    pub by_id: HashMap<String, Value>,
    pub pointer_to_id: HashMap<String, String>,
    pub by_type: HashMap<String, Vec<String>>,

    pub entity_to_pointers: HashMap<String, Vec<String>>,
}

impl EntityCache {
    fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            pointer_to_id: HashMap::new(),
            by_type: HashMap::new(),
            entity_to_pointers: HashMap::new(),
        }
    }

    fn upsert(&mut self, entity_id: String, entity_type: String, pointers: Vec<String>, entity: Value) {
        if self.by_id.contains_key(&entity_id) {
            self.remove_entity(&entity_id);
        }

        let mut owned_pointers: Vec<String> = Vec::with_capacity(pointers.len());
        for ptr in &pointers {
            let lower = ptr.to_lowercase();
            if let Some(old_id) = self.pointer_to_id.get(&lower) {
                if *old_id != entity_id {
                    let old_id = old_id.clone();
                    self.remove_pointer_from_entity(&old_id, &lower);
                }
            }
            self.pointer_to_id.insert(lower.clone(), entity_id.clone());
            owned_pointers.push(lower);
        }

        self.entity_to_pointers
            .insert(entity_id.clone(), owned_pointers);

        self.by_type
            .entry(entity_type)
            .or_default()
            .push(entity_id.clone());

        self.by_id.insert(entity_id, entity);
    }

    fn remove_entity(&mut self, entity_id: &str) {
        self.by_id.remove(entity_id);

        if let Some(pointers) = self.entity_to_pointers.remove(entity_id) {
            for ptr in pointers {
                if self
                    .pointer_to_id
                    .get(&ptr)
                    .map(|id| id == entity_id)
                    .unwrap_or(false)
                {
                    self.pointer_to_id.remove(&ptr);
                }
            }
        }

        for ids in self.by_type.values_mut() {
            ids.retain(|id| id != entity_id);
        }
    }

    fn remove_pointer_from_entity(&mut self, entity_id: &str, pointer: &str) {
        self.pointer_to_id.remove(pointer);
        if let Some(list) = self.entity_to_pointers.get_mut(entity_id) {
            list.retain(|p| p != pointer);
        }

        let still_has_pointers = self
            .entity_to_pointers
            .get(entity_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !still_has_pointers {
            self.remove_entity(entity_id);
        }
    }

    pub fn get_by_pointer(&self, pointer: &str) -> Option<&Value> {
        let id = self.pointer_to_id.get(&pointer.to_lowercase())?;
        self.by_id.get(id)
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Value> {
        self.by_id.get(id)
    }

    pub fn ids_by_type(&self, entity_type: &str) -> &[String] {
        self.by_type.get(entity_type).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn stats(&self) -> CacheStats {
        let mut by_type = HashMap::new();
        for (t, ids) in &self.by_type {
            by_type.insert(t.clone(), ids.len());
        }
        CacheStats {
            total_entities: self.by_id.len(),
            total_pointers: self.pointer_to_id.len(),
            by_type,
        }
    }
}

pub struct CacheStats {
    pub total_entities: usize,
    pub total_pointers: usize,
    pub by_type: HashMap<String, usize>,
}

pub struct ProfileLru {
    max_entries: usize,
    map: HashMap<String, Value>,
    pointer_to_id: HashMap<String, String>,

    id_to_pointers: HashMap<String, Vec<String>>,
    order: VecDeque<String>,
}

impl ProfileLru {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            map: HashMap::with_capacity(max_entries),
            pointer_to_id: HashMap::with_capacity(max_entries * 2),
            id_to_pointers: HashMap::with_capacity(max_entries),
            order: VecDeque::with_capacity(max_entries),
        }
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Value> {
        self.map.get(id)
    }

    pub fn get_by_pointer(&self, pointer: &str) -> Option<&Value> {
        let id = self.pointer_to_id.get(&pointer.to_lowercase())?;
        self.map.get(id)
    }

    pub fn insert(&mut self, entity_id: String, pointers: Vec<String>, entity: Value) {
        self.remove(&entity_id);

        while self.map.len() >= self.max_entries {
            if let Some(old_id) = self.order.pop_front() {
                self.map.remove(&old_id);

                if let Some(owned) = self.id_to_pointers.remove(&old_id) {
                    for ptr in owned {
                        if self
                            .pointer_to_id
                            .get(&ptr)
                            .map(|id| id == &old_id)
                            .unwrap_or(false)
                        {
                            self.pointer_to_id.remove(&ptr);
                        }
                    }
                }
            } else {

                break;
            }
        }

        let mut owned_lower: Vec<String> = Vec::with_capacity(pointers.len());
        for ptr in &pointers {
            let lower = ptr.to_lowercase();
            self.pointer_to_id.insert(lower.clone(), entity_id.clone());
            owned_lower.push(lower);
        }
        self.id_to_pointers.insert(entity_id.clone(), owned_lower);

        self.map.insert(entity_id.clone(), entity);
        self.order.push_back(entity_id);
    }

    pub fn remove(&mut self, entity_id: &str) {
        if self.map.remove(entity_id).is_some() {

            if let Some(owned) = self.id_to_pointers.remove(entity_id) {
                for ptr in owned {
                    if self
                        .pointer_to_id
                        .get(&ptr)
                        .map(|id| id == entity_id)
                        .unwrap_or(false)
                    {
                        self.pointer_to_id.remove(&ptr);
                    }
                }
            }
            self.order.retain(|id| id != entity_id);
        }
    }

    pub fn evict_by_pointers(&mut self, pointers: &[String]) {
        let mut to_remove = HashSet::new();
        for ptr in pointers {
            if let Some(id) = self.pointer_to_id.get(&ptr.to_lowercase()) {
                to_remove.insert(id.clone());
            }
        }
        for id in to_remove {
            self.remove(&id);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

pub struct LiveEntityCache {
    pub entities: Arc<RwLock<EntityCache>>,
    pub profiles: Arc<Mutex<ProfileLru>>,
}

impl LiveEntityCache {
    pub fn new(profile_lru_size: usize) -> Self {
        Self {
            entities: Arc::new(RwLock::new(EntityCache::new())),
            profiles: Arc::new(Mutex::new(ProfileLru::new(profile_lru_size))),
        }
    }
}

const NON_PROFILE_TYPES: &[&str] = &["scene", "wearable", "emote", "store", "outfits"];

pub async fn load_entity_cache(pool: &PgPool, cache: &LiveEntityCache) -> Result<CacheStats, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        entity_id: String,
        entity_type: String,
        entity_pointers: Vec<String>,
        entity_metadata: Option<Value>,
        entity_timestamp: f64,
        version: String,
        id: i32,
    }

    #[derive(sqlx::FromRow)]
    struct CfRow {
        deployment: i32,
        content_hash: String,
        key: String,
    }

    for entity_type in NON_PROFILE_TYPES {
        let rows: Vec<Row> = sqlx::query_as(
            r#"
            SELECT DISTINCT ON (dep.entity_id)
                dep.entity_id, dep.entity_type, dep.entity_pointers,
                dep.entity_metadata, dep.deployer_address, dep.version,
                date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
                date_part('epoch', dep.local_timestamp) * 1000 AS local_timestamp,
                dep.auth_chain, dep.id
            FROM deployments dep
            WHERE dep.entity_type = $1
              AND dep.deleter_deployment IS NULL
            ORDER BY dep.entity_id
            "#,
        )
        .bind(entity_type)
        .fetch_all(pool)
        .await?;

        if rows.is_empty() {
            continue;
        }

        let dep_ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        let cf_rows: Vec<CfRow> = sqlx::query_as(
            "SELECT deployment, content_hash, key FROM content_files WHERE deployment = ANY($1)",
        )
        .bind(&dep_ids)
        .fetch_all(pool)
        .await?;

        let mut content_map: HashMap<i32, Vec<(String, String)>> = HashMap::new();
        for cf in cf_rows {
            content_map.entry(cf.deployment).or_default().push((cf.key, cf.content_hash));
        }

        let mut ec = cache.entities.write();
        for row in rows {
            let content: Vec<Value> = content_map
                .get(&row.id)
                .unwrap_or(&vec![])
                .iter()
                .map(|(key, hash)| serde_json::json!({"file": key, "hash": hash}))
                .collect();

            let metadata = row.entity_metadata.as_ref().and_then(|m| m.get("v").cloned());

            let Some(timestamp) = checked_f64_to_i64(row.entity_timestamp) else {
                tracing::warn!(
                    entity_id = %row.entity_id,
                    raw_timestamp = row.entity_timestamp,
                    "Skipping row with non-finite/out-of-range entity_timestamp"
                );
                continue;
            };

            let entity = serde_json::json!({
                "version": row.version,
                "id": row.entity_id,
                "type": row.entity_type,
                "timestamp": timestamp,
                "pointers": row.entity_pointers,
                "content": content,
                "metadata": metadata,
            });

            let pointers = row.entity_pointers.clone();
            ec.upsert(row.entity_id, row.entity_type.to_string(), pointers, entity);
        }
    }

    let stats = cache.entities.read().stats();
    Ok(stats)
}

#[derive(Deserialize)]
struct NotifyPayload {
    #[serde(rename = "type")]
    entity_type: String,
    id: String,
    pointers: Vec<String>,
}

pub const CREATE_TRIGGER_SQL: &str = r#"
CREATE OR REPLACE FUNCTION notify_new_deployment() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('new_deployment', json_build_object(
        'type', NEW.entity_type,
        'id', NEW.entity_id,
        'pointers', to_json(NEW.entity_pointers)
    )::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS deployment_notify_trigger ON deployments;
CREATE TRIGGER deployment_notify_trigger
    AFTER INSERT ON deployments
    FOR EACH ROW EXECUTE FUNCTION notify_new_deployment();
"#;

pub fn spawn_invalidation_listener(
    pool: PgPool,
    cache: Arc<LiveEntityCache>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_listener(&pool, &cache).await {
                Ok(()) => break,
                Err(e) => {
                    tracing::warn!(error = %e, "NOTIFY listener disconnected, reconnecting in 1s");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    })
}

async fn run_listener(pool: &PgPool, cache: &LiveEntityCache) -> Result<(), sqlx::Error> {
    let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
    listener.listen("new_deployment").await?;
    tracing::info!("Listening for deployment notifications on 'new_deployment' channel");

    loop {
        let notification = listener.recv().await?;
        let payload_str = notification.payload();

        let payload: NotifyPayload = match serde_json::from_str(payload_str) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(payload = payload_str, error = %e, "Invalid NOTIFY payload");
                continue;
            }
        };

        if payload.entity_type == "profile" {
            handle_profile_invalidation(&cache.profiles, &payload);
        } else {
            handle_entity_invalidation(pool, &cache.entities, &payload).await;
        }
    }
}

fn handle_profile_invalidation(profiles: &Mutex<ProfileLru>, payload: &NotifyPayload) {
    let mut lru = profiles.lock();

    lru.remove(&payload.id);

    lru.evict_by_pointers(&payload.pointers);

    tracing::debug!(
        entity_id = %payload.id,
        pointers = ?payload.pointers,
        "Profile cache: evicted"
    );
}

async fn handle_entity_invalidation(
    pool: &PgPool,
    entities: &RwLock<EntityCache>,
    payload: &NotifyPayload,
) {
    let displaced_ids: Vec<String> = {
        let ec = entities.read();
        let mut ids = HashSet::new();
        for ptr in &payload.pointers {
            if let Some(old_id) = ec.pointer_to_id.get(&ptr.to_lowercase()) {
                if *old_id != payload.id {
                    ids.insert(old_id.clone());
                }
            }
        }
        ids.into_iter().collect()
    };

    #[derive(sqlx::FromRow)]
    struct PtrRow {
        pointer: String,
        entity_id: String,
    }

    let current_mappings: Vec<PtrRow> = match sqlx::query_as(
        "SELECT pointer, entity_id FROM active_pointers WHERE pointer = ANY($1)",
    )
    .bind(&payload.pointers)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to query active_pointers for invalidation");
            return;
        }
    };

    let mut entities_to_fetch: HashSet<String> = HashSet::new();
    entities_to_fetch.insert(payload.id.clone());

    for displaced_id in &displaced_ids {
        let still_active = current_mappings.iter().any(|m| m.entity_id == *displaced_id);
        if still_active {
            entities_to_fetch.insert(displaced_id.clone());
        }
    }

    #[derive(sqlx::FromRow)]
    struct EntRow {
        entity_id: String,
        entity_type: String,
        entity_pointers: Vec<String>,
        entity_metadata: Option<Value>,
        entity_timestamp: f64,
        version: String,
        id: i32,
    }

    #[derive(sqlx::FromRow)]
    struct CfRow {
        deployment: i32,
        content_hash: String,
        key: String,
    }

    let fetch_ids: Vec<String> = entities_to_fetch.into_iter().collect();
    let fresh_rows: Vec<EntRow> = match sqlx::query_as(
        r#"
        SELECT entity_id, entity_type, entity_pointers, entity_metadata,
               date_part('epoch', entity_timestamp) * 1000 AS entity_timestamp,
               version, id
        FROM deployments
        WHERE entity_id = ANY($1) AND deleter_deployment IS NULL
        "#,
    )
    .bind(&fetch_ids)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to re-fetch entities for invalidation");
            return;
        }
    };

    let dep_ids: Vec<i32> = fresh_rows.iter().map(|r| r.id).collect();
    let cf_rows: Vec<CfRow> = if dep_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as("SELECT deployment, content_hash, key FROM content_files WHERE deployment = ANY($1)")
            .bind(&dep_ids)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
    };

    let mut content_map: HashMap<i32, Vec<(String, String)>> = HashMap::new();
    for cf in cf_rows {
        content_map.entry(cf.deployment).or_default().push((cf.key, cf.content_hash));
    }

    let mut ec = entities.write();

    for displaced_id in &displaced_ids {
        let still_in_fresh = fresh_rows.iter().any(|r| r.entity_id == *displaced_id);
        if !still_in_fresh {
            ec.remove_entity(displaced_id);
            tracing::debug!(entity_id = %displaced_id, "Entity removed from cache (overwritten)");
        }
    }

    for row in fresh_rows {
        let content: Vec<Value> = content_map
            .get(&row.id)
            .unwrap_or(&vec![])
            .iter()
            .map(|(key, hash)| serde_json::json!({"file": key, "hash": hash}))
            .collect();

        let metadata = row.entity_metadata.as_ref().and_then(|m| m.get("v").cloned());

        let Some(timestamp) = checked_f64_to_i64(row.entity_timestamp) else {
            tracing::warn!(
                entity_id = %row.entity_id,
                raw_timestamp = row.entity_timestamp,
                "Skipping invalidation row with non-finite/out-of-range entity_timestamp"
            );
            continue;
        };

        let entity = serde_json::json!({
            "version": row.version,
            "id": row.entity_id,
            "type": row.entity_type,
            "timestamp": timestamp,
            "pointers": row.entity_pointers,
            "content": content,
            "metadata": metadata,
        });

        let pointers = row.entity_pointers.clone();
        let etype = row.entity_type.clone();
        tracing::debug!(entity_id = %row.entity_id, entity_type = %etype, "Entity cache: upserted");
        ec.upsert(row.entity_id, etype, pointers, entity);
    }

    for mapping in &current_mappings {
        let lower = mapping.pointer.to_lowercase();

        if let Some(prev_id) = ec.pointer_to_id.get(&lower).cloned() {
            if prev_id != mapping.entity_id {
                if let Some(list) = ec.entity_to_pointers.get_mut(&prev_id) {
                    list.retain(|p| p != &lower);
                }
            }
        }
        ec.pointer_to_id.insert(lower.clone(), mapping.entity_id.clone());
        let owned = ec
            .entity_to_pointers
            .entry(mapping.entity_id.clone())
            .or_default();
        if !owned.iter().any(|p| p == &lower) {
            owned.push(lower);
        }
    }
}
