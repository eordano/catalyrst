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
    /// A set, not a Vec: as a Vec the membership check made a bulk load quadratic in the type's size
    /// -- ~45k wearables meant ~1e9 string comparisons, the bulk of cache-load wall time. Nothing
    /// reads this index today; keeping it a set means the first reader inherits O(1).
    by_type: HashMap<&'static str, HashSet<String>>,
}

impl EntityCache {
    pub(crate) fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            pointer_to_id: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    fn upsert(&mut self, entity: CachedEntity, active_pointers: Vec<String>) {
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

        for ptr in active_pointers {
            self.pointer_to_id
                .insert(ptr.to_lowercase(), entity.entity_id.clone());
        }

        let etype = entity.entity_type;
        let eid = entity.entity_id.clone();
        self.by_id.insert(entity.entity_id.clone(), entity);

        self.by_type.entry(etype).or_default().insert(eid);
    }

    /// Folds a separately-loaded cache in, one `upsert` per entity so the
    /// pointer-reassignment rules are exactly those of a sequential load.
    pub(crate) fn absorb(&mut self, other: EntityCache) {
        let mut pointers: HashMap<String, Vec<String>> = HashMap::new();
        for (pointer, id) in other.pointer_to_id {
            pointers.entry(id).or_default().push(pointer);
        }
        for (id, entity) in other.by_id {
            self.upsert(entity, pointers.remove(&id).unwrap_or_default());
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
            if let Some(type_set) = self.by_type.get_mut(old.entity_type) {
                type_set.remove(entity_id);
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

#[derive(sqlx::FromRow)]
struct CachedEntityRow {
    #[sqlx(flatten)]
    entity: ActiveEntityRow,
    active_pointers: Vec<String>,
}

pub(crate) async fn load_entity_type_into_cache(
    pool: &PgPool,
    cache: &mut EntityCache,
    entity_type: &str,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<CachedEntityRow> = sqlx::query_as(
        r#"
        SELECT
            dep.entity_id,
            dep.entity_type,
            dep.entity_pointers,
            dep.entity_metadata,
            date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
            dep.version,
            dep.id,
            array_agg(ap.pointer::text) AS active_pointers,
            COALESCE(
                (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key)
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
        let entity = row_to_cached_entity(row.entity);
        cache.upsert(entity, row.active_pointers);
    }
    Ok(count)
}

pub(crate) async fn invalidate_deployment_caches(
    pool: &PgPool,
    entity_cache: &Arc<RwLock<EntityCache>>,
    profile_lru: &Arc<Mutex<ProfileLru>>,
    prefix_ids_cache: &Arc<Mutex<PrefixIdsCache>>,
    entity_id: &str,
) -> Result<(), sqlx::Error> {
    // Cache reads hold this barrier through any database fetch and cache fill.
    // Eviction therefore cannot race with a stale in-flight query refilling it.
    let mut cache = entity_cache.write().await;
    let affected: Result<Vec<(String, Vec<String>)>, _> = sqlx::query_as(
        r#"
        SELECT entity_id, entity_pointers FROM deployments WHERE entity_id = $1
        UNION ALL
        SELECT old.entity_id, old.entity_pointers
        FROM deployments old
        INNER JOIN deployments incoming ON old.deleter_deployment = incoming.id
        WHERE incoming.entity_id = $1
        "#,
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await;
    let mut profiles = profile_lru.lock().await;
    let mut prefixes = prefix_ids_cache.lock().await;
    let affected = match affected {
        Ok(rows) => rows,
        Err(error) => {
            cache.by_id.clear();
            cache.pointer_to_id.clear();
            cache.by_type.clear();
            profiles.map.clear();
            profiles.order.clear();
            prefixes.map.clear();
            prefixes.order.clear();
            return Err(error);
        }
    };
    let pointers: Vec<String> = affected
        .iter()
        .flat_map(|(_, pointers)| pointers.iter().map(|p| p.to_lowercase()))
        .collect();
    let mut ids: HashSet<String> = affected.into_iter().map(|(id, _)| id).collect();
    ids.insert(entity_id.to_string());
    for pointer in &pointers {
        if let Some(id) = cache.pointer_to_id.get(pointer) {
            ids.insert(id.clone());
        }
    }
    // Evict whole entities: replacing one parcel also clears any other parcels
    // from the superseded scene.
    for id in ids {
        cache.remove(&id);
        profiles.remove(&id);
    }
    prefixes.remove_matching(&pointers);
    let tombstone_filter = if catalyrst_server::land_publish::local_entities_present(pool).await {
        "AND NOT EXISTS (SELECT 1 FROM local_entities le
         WHERE le.entity_id = dep.entity_id AND le.tombstoned_at IS NOT NULL)"
    } else {
        ""
    };
    // Read current ownership under the same barrier. Even an old notification
    // refreshes the latest database state, rather than its original deployment.
    let refreshed: Vec<CachedEntityRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        r#"
        SELECT dep.entity_id, dep.entity_type, dep.entity_pointers, dep.entity_metadata,
               date_part('epoch', dep.entity_timestamp) * 1000 AS entity_timestamp,
               dep.version, dep.id,
               array_agg(ap.pointer::text) AS active_pointers,
               COALESCE(
                   (SELECT json_agg(json_build_object('key', cf.key, 'hash', cf.content_hash) ORDER BY cf.key)
                    FROM content_files cf WHERE cf.deployment = dep.id),
                   '[]'::json
               ) AS content_json
        FROM deployments dep
        INNER JOIN active_pointers ap ON ap.entity_id = dep.entity_id
        WHERE dep.entity_id IN (SELECT entity_id FROM active_pointers WHERE pointer = ANY($1))
          AND dep.entity_type = ANY($2)
          AND dep.deleter_deployment IS NULL
          {tombstone_filter}
        GROUP BY dep.id
        "#
    )))
    .bind(&pointers)
    .bind(CACHED_ENTITY_TYPES)
    .fetch_all(pool)
    .await?;
    for row in refreshed {
        cache.upsert(row_to_cached_entity(row.entity), row.active_pointers);
    }
    Ok(())
}

pub(crate) async fn install_notify_trigger(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        CREATE OR REPLACE FUNCTION notify_new_deployment() RETURNS trigger AS $$
        BEGIN
            PERFORM pg_notify('new_deployment', NEW.entity_type || ':' || NEW.entity_id);
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"
        DROP TRIGGER IF EXISTS deployment_notify_trigger ON deployments;
        "#
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"
        CREATE TRIGGER deployment_notify_trigger
            AFTER INSERT ON deployments
            FOR EACH ROW EXECUTE FUNCTION notify_new_deployment();
        "#
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
            tracing::error!(error = %e, "Failed to create PgListener -- cache invalidation disabled");
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
                tracing::warn!(error = %e, "PgListener recv error -- reconnecting");
                continue;
            }
        };

        let payload = notification.payload();
        let Some((entity_type, entity_id)) = payload.split_once(':') else {
            tracing::warn!(payload, "Malformed NOTIFY payload -- expected 'type:id'");
            continue;
        };

        if entity_type == "profile" || CACHED_ENTITY_TYPES.contains(&entity_type) {
            if let Err(error) = invalidate_deployment_caches(
                &pool,
                &entity_cache,
                &profile_lru,
                &prefix_ids_cache,
                entity_id,
            )
            .await
            {
                tracing::warn!(entity_id, %error, "Failed deployment cache refresh; affected entries evicted");
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

    #[test]
    fn deployment_visibility_preserves_authoritative_pointer_ownership_during_absorb() {
        use super::*;
        let mut partial = EntityCache::new();
        partial.upsert(
            CachedEntity {
                entity_id: "old".into(),
                entity_type: "scene",
                pointers: vec!["0,0".into(), "1,0".into()],
                bytes: Bytes::from_static(b"{}"),
            },
            vec!["0,0".into()],
        );
        let mut cache = EntityCache::new();
        cache.absorb(partial);
        assert_eq!(
            cache.pointer_to_id.get("0,0").map(String::as_str),
            Some("old")
        );
        assert!(!cache.pointer_to_id.contains_key("1,0"));
        cache.upsert(
            CachedEntity {
                entity_id: "new".into(),
                entity_type: "scene",
                pointers: vec!["1,0".into()],
                bytes: Bytes::from_static(b"{}"),
            },
            vec!["1,0".into()],
        );
        cache.remove("old");
        assert_eq!(
            cache.pointer_to_id.get("1,0").map(String::as_str),
            Some("new")
        );
    }

    async fn visibility_fixture() -> Option<(super::LiveDatabase, String)> {
        use super::*;
        let url = catalyrst_testgate::require_pg("CATALYRST_SERVER_TEST_PG")?;
        let schema = format!("test_visibility_{}", uuid::Uuid::new_v4().simple());
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        let options: PgConnectOptions = url.parse().unwrap();
        let options = options.options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap();
        admin.close().await;
        for ddl in [
            "CREATE TABLE deployments (id integer PRIMARY KEY, entity_id text UNIQUE NOT NULL,
             entity_type text NOT NULL, entity_pointers text[] NOT NULL, entity_metadata json,
             entity_timestamp timestamp NOT NULL DEFAULT now(), version text NOT NULL DEFAULT 'v3',
             deleter_deployment integer)",
            "CREATE TABLE active_pointers (pointer text PRIMARY KEY, entity_id text NOT NULL)",
            "CREATE TABLE content_files (deployment integer, key text, content_hash text)",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(ddl))
                .execute(&pool)
                .await
                .unwrap();
        }
        let db = LiveDatabase {
            pool,
            entity_cache: Arc::new(RwLock::new(EntityCache::new())),
            profile_lru: Arc::new(Mutex::new(ProfileLru::new(10))),
            prefix_ids_cache: Arc::new(Mutex::new(PrefixIdsCache::new(
                10,
                Duration::from_secs(60),
            ))),
        };
        Some((db, schema))
    }

    async fn drop_visibility_fixture(db: super::LiveDatabase, schema: String) {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&db.pool)
            .await
            .unwrap();
        db.pool.close().await;
    }

    #[tokio::test]
    async fn deployment_visibility_evicts_superseded_ids_and_removed_parcels() {
        use super::*;
        let Some((db, schema)) = visibility_fixture().await else {
            return;
        };
        sqlx::query(
            "INSERT INTO deployments (id, entity_id, entity_type, entity_pointers)
                     VALUES (1, 'old', 'scene', ARRAY['0,0', '1,0']),
                            (2, 'unrelated', 'scene', ARRAY['9,9'])",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO active_pointers VALUES ('0,0', 'old'), ('1,0', 'old'), ('9,9', 'unrelated')")
            .execute(&db.pool).await.unwrap();
        load_entity_type_into_cache(&db.pool, &mut *db.entity_cache.write().await, "scene")
            .await
            .unwrap();
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO deployments (id, entity_id, entity_type, entity_pointers)
                     VALUES (3, 'new', 'scene', ARRAY['0,0'])",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("UPDATE deployments SET deleter_deployment = 3 WHERE id = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE active_pointers SET entity_id = 'new' WHERE pointer = '0,0'")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DELETE FROM active_pointers WHERE pointer = '1,0'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // No listener is running: this deterministically recreates the interval
        // between commit and asynchronous notification processing.
        assert_eq!(
            db.active_entities_by_pointers(&["0,0".into()])
                .await
                .unwrap()[0]["id"],
            "old"
        );
        db.deployment_committed("new").await.unwrap();
        assert!(db.entity_cache.read().await.by_id.contains_key("new"));
        assert_eq!(
            db.active_entities_by_pointers(&["0,0".into()])
                .await
                .unwrap()[0]["id"],
            "new"
        );
        assert!(db
            .active_entities_by_pointers(&["1,0".into()])
            .await
            .unwrap()
            .is_empty());
        assert!(db
            .active_entities_by_ids(&["old".into()])
            .await
            .unwrap()
            .is_empty());
        assert!(db.entity_cache.read().await.by_id.contains_key("unrelated"));
        // A delayed notification for the old deployment must not resurrect it.
        db.deployment_committed("old").await.unwrap();
        assert_eq!(
            db.active_entities_by_pointers(&["0,0".into()])
                .await
                .unwrap()[0]["id"],
            "new"
        );
        assert!(db
            .active_entities_by_pointers(&["1,0".into()])
            .await
            .unwrap()
            .is_empty());
        let mut reloaded = EntityCache::new();
        load_entity_type_into_cache(&db.pool, &mut reloaded, "scene")
            .await
            .unwrap();
        assert!(!reloaded.by_id.contains_key("old"));
        drop_visibility_fixture(db, schema).await;
    }

    #[tokio::test]
    async fn deployment_visibility_refresh_failure_preserves_commit_and_evicts_stale_entries() {
        use super::*;
        let Some((db, schema)) = visibility_fixture().await else {
            return;
        };
        sqlx::query(
            "INSERT INTO deployments (id, entity_id, entity_type, entity_pointers)
                     VALUES (1, 'old', 'scene', ARRAY['0,0']),
                            (2, 'new', 'scene', ARRAY['0,0']),
                            (3, 'unrelated', 'scene', ARRAY['9,9'])",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO active_pointers VALUES ('0,0', 'old'), ('9,9', 'unrelated')")
            .execute(&db.pool)
            .await
            .unwrap();
        load_entity_type_into_cache(&db.pool, &mut *db.entity_cache.write().await, "scene")
            .await
            .unwrap();
        sqlx::query("UPDATE deployments SET deleter_deployment = 2 WHERE id = 1")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE active_pointers SET entity_id = 'new' WHERE pointer = '0,0'")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE content_files")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_committed("new")
            .await
            .expect("refill failure must not reject a committed deployment");
        {
            let cache = db.entity_cache.read().await;
            assert!(!cache.by_id.contains_key("old"));
            assert!(!cache.pointer_to_id.contains_key("0,0"));
            assert!(cache.by_id.contains_key("unrelated"));
        }
        assert!(
            db.active_entities_by_pointers(&["0,0".into()])
                .await
                .is_err(),
            "a database failure must not serve the evicted stale scene"
        );
        db.profile_lru
            .lock()
            .await
            .insert("profile".into(), json!({"id": "profile"}));
        db.prefix_ids_cache
            .lock()
            .await
            .insert("prefix".into(), ids(&["old"]));
        sqlx::query("DROP TABLE deployments")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_committed("new")
            .await
            .expect("affected-lookup failure must not reject a committed deployment");
        assert!(db.entity_cache.read().await.by_id.is_empty());
        assert!(db.entity_cache.read().await.pointer_to_id.is_empty());
        assert!(db.profile_lru.lock().await.map.is_empty());
        assert!(db.prefix_ids_cache.lock().await.map.is_empty());
        drop_visibility_fixture(db, schema).await;
    }

    #[tokio::test]
    async fn deployment_visibility_waits_for_inflight_profile_and_prefix_fills() {
        use super::*;
        let Some((db, schema)) = visibility_fixture().await else {
            return;
        };
        let pointer = "urn:decentraland:matic:collections-v2:0xaaa:1";
        sqlx::query("INSERT INTO deployments (id, entity_id, entity_type, entity_pointers, deleter_deployment)
                     VALUES (1, 'old', 'wearable', ARRAY[$1], 2), (2, 'new', 'wearable', ARRAY[$1], NULL)")
            .bind(pointer).execute(&db.pool).await.unwrap();
        let cache = db.entity_cache.clone();
        let profile_lru = db.profile_lru.clone();
        let prefix_cache = db.prefix_ids_cache.clone();
        let pool = db.pool.clone();
        let read_guard = db.entity_cache.read().await;
        let task = tokio::spawn(async move {
            invalidate_deployment_caches(&pool, &cache, &profile_lru, &prefix_cache, "new")
                .await
                .unwrap();
        });
        tokio::pin!(task);
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut task)
            .await
            .is_err());
        // These are stale results returned by a read started before the commit.
        db.profile_lru
            .lock()
            .await
            .insert("old".into(), json!({"id": "old"}));
        db.prefix_ids_cache.lock().await.insert(
            "urn:decentraland:matic:collections-v2:0xaaa".into(),
            ids(&["old"]),
        );
        db.prefix_ids_cache
            .lock()
            .await
            .insert("unrelated".into(), ids(&["other"]));
        drop(read_guard);
        tokio::time::timeout(Duration::from_secs(5), &mut task)
            .await
            .unwrap()
            .unwrap();
        assert!(db.profile_lru.lock().await.get("old").is_none());
        let prefixes = db.prefix_ids_cache.lock().await;
        assert!(prefixes
            .get("urn:decentraland:matic:collections-v2:0xaaa")
            .is_none());
        assert!(prefixes.get("unrelated").is_some());
        drop(prefixes);
        drop_visibility_fixture(db, schema).await;
    }

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
