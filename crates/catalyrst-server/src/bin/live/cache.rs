use super::*;

#[derive(Clone)]
pub(crate) struct CachedEntity {
    pub(crate) entity_id: String,
    pub(crate) entity_type: &'static str,
    pub(crate) pointers: Vec<String>,
    pub(crate) bytes: Bytes,
}

pub(crate) struct EntityCache {
    pub(crate) by_id: HashMap<String, CachedEntity>,
    pub(crate) pointer_to_id: HashMap<String, String>,
    by_type: HashMap<&'static str, Vec<String>>,
}

impl EntityCache {
    pub(crate) fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            pointer_to_id: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    fn upsert(&mut self, entity: CachedEntity) {
        if let Some(old) = self.by_id.get(&entity.entity_id) {
            for ptr in &old.pointers {
                if self
                    .pointer_to_id
                    .get(ptr)
                    .map(|id| id == &entity.entity_id)
                    .unwrap_or(false)
                {
                    self.pointer_to_id.remove(ptr);
                }
            }
        }

        for ptr in &entity.pointers {
            self.pointer_to_id
                .insert(ptr.clone(), entity.entity_id.clone());
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
                if self
                    .pointer_to_id
                    .get(ptr)
                    .map(|id| id == entity_id)
                    .unwrap_or(false)
                {
                    self.pointer_to_id.remove(ptr);
                }
            }
            if let Some(type_vec) = self.by_type.get_mut(old.entity_type) {
                type_vec.retain(|id| id != entity_id);
            }
        }
    }
}

pub(crate) struct ProfileLru {
    map: HashMap<String, (Instant, Value)>,
    order: VecDeque<String>,
    max_entries: usize,
}

impl ProfileLru {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            map: HashMap::with_capacity(max_entries),
            order: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub(crate) fn get(&self, entity_id: &str) -> Option<&Value> {
        self.map.get(entity_id).map(|(_, v)| v)
    }

    pub(crate) fn insert(&mut self, entity_id: String, value: Value) {
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

pub(crate) struct PrefixIdsCache {
    map: HashMap<String, (Instant, Arc<Vec<String>>)>,
    order: VecDeque<String>,
    max_entries: usize,
    ttl: std::time::Duration,
}

impl PrefixIdsCache {
    pub(crate) fn new(max_entries: usize, ttl: std::time::Duration) -> Self {
        Self {
            map: HashMap::with_capacity(max_entries),
            order: VecDeque::with_capacity(max_entries),
            max_entries,
            ttl,
        }
    }

    pub(crate) fn get(&self, prefix: &str) -> Option<Arc<Vec<String>>> {
        let (inserted, ids) = self.map.get(prefix)?;
        if inserted.elapsed() >= self.ttl {
            return None;
        }
        Some(ids.clone())
    }

    pub(crate) fn insert(&mut self, prefix: String, ids: Arc<Vec<String>>) {
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

    pub(crate) fn remove_matching(&mut self, pointers: &[String]) {
        let lowered: Vec<String> = pointers.iter().map(|p| p.to_lowercase()).collect();
        self.map.retain(|prefix, _| {
            let prefix_lower = prefix.to_lowercase();
            !lowered.iter().any(|p| p.starts_with(&prefix_lower))
        });
        let map = &self.map;
        self.order.retain(|p| map.contains_key(p));
    }
}

const CACHED_ENTITY_TYPES: &[&str] = &["scene", "wearable", "emote", "store", "outfits"];

pub(crate) async fn load_entity_type_into_cache(
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
                (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.ctid)
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
) -> Vec<String> {
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
                (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.ctid)
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
            return Vec::new();
        }
    };

    let mut cache = cache.write().await;
    let mut affected: Vec<String> = cache
        .by_id
        .get(entity_id)
        .map(|e| e.pointers.clone())
        .unwrap_or_default();
    match row {
        Some(row) => {
            let entity = row_to_cached_entity(row);
            for ptr in &entity.pointers {
                if !affected.contains(ptr) {
                    affected.push(ptr.clone());
                }
            }
            cache.upsert(entity);
            tracing::debug!(entity_id, entity_type, "Cache: refreshed entity");
        }
        None => {
            cache.remove(entity_id);
            tracing::debug!(entity_id, entity_type, "Cache: removed deleted entity");
        }
    }
    affected
}

pub(crate) async fn install_notify_trigger(pool: &PgPool) -> Result<(), sqlx::Error> {
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

pub(crate) async fn listen_for_invalidations(
    pool: PgPool,
    entity_cache: Arc<RwLock<EntityCache>>,
    profile_lru: Arc<Mutex<ProfileLru>>,
    prefix_ids_cache: Arc<Mutex<PrefixIdsCache>>,
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
                    let affected =
                        refresh_entity_in_cache(&pool, &entity_cache, entity_type, entity_id).await;
                    if matches!(entity_type, "wearable" | "emote") && !affected.is_empty() {
                        prefix_ids_cache.lock().await.remove_matching(&affected);
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn is_cached_type(entity_type: &str) -> bool {
    CACHED_ENTITY_TYPES.contains(&entity_type)
}

#[cfg(test)]
mod tests {
    use super::PrefixIdsCache;
    use std::sync::Arc;
    use std::time::Duration;

    fn ids(v: &[&str]) -> Arc<Vec<String>> {
        Arc::new(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn remove_matching_drops_prefixes_covering_pointers() {
        let mut cache = PrefixIdsCache::new(10, Duration::from_secs(60));
        cache.insert(
            "urn:decentraland:matic:collections-v2:0xaaa".to_string(),
            ids(&["id1"]),
        );
        cache.insert(
            "urn:decentraland:matic:collections-v2:0xbbb".to_string(),
            ids(&["id2"]),
        );

        cache.remove_matching(&["urn:decentraland:matic:collections-v2:0xaaa:3".to_string()]);

        assert!(cache
            .get("urn:decentraland:matic:collections-v2:0xaaa")
            .is_none());
        assert!(cache
            .get("urn:decentraland:matic:collections-v2:0xbbb")
            .is_some());
    }

    #[test]
    fn remove_matching_is_case_insensitive() {
        let mut cache = PrefixIdsCache::new(10, Duration::from_secs(60));
        cache.insert(
            "urn:decentraland:matic:collections-v2:0xAAA".to_string(),
            ids(&["id1"]),
        );

        cache.remove_matching(&["urn:decentraland:matic:collections-v2:0xaaa:1".to_string()]);

        assert!(cache
            .get("urn:decentraland:matic:collections-v2:0xAAA")
            .is_none());
    }

    #[test]
    fn remove_matching_ignores_unrelated_pointers() {
        let mut cache = PrefixIdsCache::new(10, Duration::from_secs(60));
        cache.insert(
            "urn:decentraland:matic:collections-v2:0xaaa".to_string(),
            ids(&["id1"]),
        );

        cache.remove_matching(&["urn:decentraland:off-chain:base-avatars:eyes_00".to_string()]);

        assert!(cache
            .get("urn:decentraland:matic:collections-v2:0xaaa")
            .is_some());
    }
}
