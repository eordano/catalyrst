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
    /// Bumped under the write lock by every mutation, so a reader that dropped its read guard
    /// before hitting SQL can tell whether a deployment landed in between and skip filling the
    /// side caches with what it read.
    pub(crate) generation: u64,
}

impl EntityCache {
    pub(crate) fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            pointer_to_id: HashMap::new(),
            by_type: HashMap::new(),
            generation: 0,
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
        self.generation += 1;
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
    map: HashMap<String, (Instant, Bytes)>,
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

    pub(crate) fn get(&self, entity_id: &str) -> Option<&Bytes> {
        self.map.get(entity_id).map(|(_, v)| v)
    }

    pub(crate) fn insert(&mut self, entity_id: String, value: Bytes) {
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

const AFFECTED_DEPLOYMENTS: &str = "
    SELECT entity_id, entity_pointers FROM deployments WHERE entity_id = $1
    UNION ALL
    SELECT old.entity_id, old.entity_pointers
    FROM deployments old
    INNER JOIN deployments incoming ON old.deleter_deployment = incoming.id
    WHERE incoming.entity_id = $1";

type RefreshScope = (HashSet<String>, Vec<String>);

fn decode_refresh(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<(RefreshScope, Vec<CachedEntityRow>), sqlx::Error> {
    use sqlx::{FromRow, Row};
    let mut scope = (HashSet::new(), Vec::new());
    let mut entities = Vec::new();
    for row in rows {
        if let Some(ids) = row.try_get::<Option<Vec<String>>, _>("affected_ids")? {
            scope = (ids.into_iter().collect(), row.try_get("affected_pointers")?);
        }
        if row.try_get::<Option<String>, _>("entity_id")?.is_some() {
            entities.push(CachedEntityRow::from_row(&row)?);
        }
    }
    Ok((scope, entities))
}

pub(crate) async fn invalidate_deployment_caches(
    pool: &PgPool,
    entity_cache: &Arc<RwLock<EntityCache>>,
    profile_lru: &Arc<Mutex<ProfileLru>>,
    prefix_ids_cache: &Arc<Mutex<PrefixIdsCache>>,
    entity_id: &str,
    local_entities: bool,
) -> Result<(), sqlx::Error> {
    // Hold the same barrier across the database snapshot and cache replacement.
    let mut cache = entity_cache.write().await;
    cache.generation += 1;
    let tombstone_filter = if local_entities {
        "AND NOT EXISTS (SELECT 1 FROM local_entities le
         WHERE le.entity_id = dep.entity_id AND le.tombstoned_at IS NOT NULL)"
    } else {
        ""
    };
    let snapshot = sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"
        WITH affected AS MATERIALIZED ({AFFECTED_DEPLOYMENTS}),
        pointers AS (SELECT DISTINCT lower(unnest(entity_pointers)) AS pointer FROM affected),
        refreshed AS (
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
            WHERE dep.entity_id IN (
                SELECT entity_id FROM active_pointers WHERE pointer IN (SELECT pointer FROM pointers)
            )
              AND dep.entity_type = ANY($2)
              AND dep.deleter_deployment IS NULL
              {tombstone_filter}
            GROUP BY dep.id
        )
        SELECT CASE WHEN row_number() OVER () = 1
                    THEN ARRAY(SELECT entity_id FROM affected) END AS affected_ids,
               CASE WHEN row_number() OVER () = 1
                    THEN ARRAY(SELECT pointer FROM pointers) END AS affected_pointers,
               refreshed.*
        FROM (SELECT 1) marker LEFT JOIN refreshed ON true
        "#
    )))
    .bind(entity_id)
    .bind(CACHED_ENTITY_TYPES)
    .fetch_all(pool)
    .await
    .and_then(decode_refresh);
    let (scope, refreshed, refresh_error) = match snapshot {
        Ok((scope, rows)) => (Ok(scope), rows, None),
        Err(error) => {
            // A failed refill must still evict stale entries. The rare failure path
            // keeps the narrower eviction when the affected-entity lookup still works.
            let affected: Result<Vec<(String, Vec<String>)>, _> =
                sqlx::query_as(AFFECTED_DEPLOYMENTS)
                    .bind(entity_id)
                    .fetch_all(pool)
                    .await;
            let scope = affected.map(|rows| {
                let pointers = rows
                    .iter()
                    .flat_map(|(_, p)| p.iter().map(|p| p.to_lowercase()))
                    .collect();
                (rows.into_iter().map(|(id, _)| id).collect(), pointers)
            });
            (scope, Vec::new(), Some(error))
        }
    };
    let mut profiles = profile_lru.lock().await;
    let mut prefixes = prefix_ids_cache.lock().await;
    let (mut ids, pointers) = match scope {
        Ok(scope) => scope,
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
    ids.insert(entity_id.to_string());
    for pointer in &pointers {
        if let Some(id) = cache.pointer_to_id.get(pointer) {
            ids.insert(id.clone());
        }
    }
    for id in ids {
        cache.remove(&id);
        profiles.remove(&id);
    }
    prefixes.remove_matching(&pointers);
    for row in refreshed {
        cache.upsert(row_to_cached_entity(row.entity), row.active_pointers);
    }
    match refresh_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
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
    local_entities: bool,
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
                local_entities,
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
        let deployment_schema =
            catalyrst_server::schema_migrations::DeploymentSchema::detect(&pool)
                .await
                .unwrap();
        let db = LiveDatabase {
            pool,
            deployment_schema,
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
        db.profile_lru.lock().await.insert(
            "profile".into(),
            Bytes::from_static(b"{\"id\":\"profile\"}"),
        );
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
            invalidate_deployment_caches(&pool, &cache, &profile_lru, &prefix_cache, "new", false)
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
            .insert("old".into(), Bytes::from_static(b"{\"id\":\"old\"}"));
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

    #[tokio::test]
    async fn deployment_visibility_combines_multiple_successors_and_honors_tombstones() {
        use super::*;
        let Some((mut db, schema)) = visibility_fixture().await else {
            return;
        };
        assert!(!db.deployment_schema.local_entities);
        assert!(!db.deployment_schema.pointer_entity_type);
        sqlx::query("INSERT INTO deployments (id, entity_id, entity_type, entity_pointers, deleter_deployment)
                     VALUES (1, 'old', 'scene', ARRAY['0,0', '1,0', '2,0'], 2),
                            (2, 'a', 'scene', ARRAY['0,0'], NULL),
                            (3, 'b', 'scene', ARRAY['1,0'], NULL)")
            .execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO active_pointers VALUES ('0,0', 'a'), ('1,0', 'b')")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_committed("old").await.unwrap();
        {
            let cache = db.entity_cache.read().await;
            assert!(cache.by_id.contains_key("a") && cache.by_id.contains_key("b"));
            assert_eq!(cache.pointer_to_id.len(), 2);
            assert!(!cache.pointer_to_id.contains_key("2,0"));
        }
        sqlx::query(
            "CREATE TABLE local_entities (entity_id text PRIMARY KEY, tombstoned_at timestamp)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("ALTER TABLE active_pointers ADD COLUMN entity_type text")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_schema =
            catalyrst_server::schema_migrations::DeploymentSchema::detect(&db.pool)
                .await
                .unwrap();
        assert!(db.deployment_schema.local_entities && db.deployment_schema.pointer_entity_type);
        sqlx::query("INSERT INTO local_entities VALUES ('a', now())")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_committed("old").await.unwrap();
        {
            let cache = db.entity_cache.read().await;
            assert!(!cache.by_id.contains_key("a"));
            assert!(!cache.pointer_to_id.contains_key("0,0"));
            assert!(cache.by_id.contains_key("b"));
        }
        sqlx::query("INSERT INTO local_entities VALUES ('b', now())")
            .execute(&db.pool)
            .await
            .unwrap();
        db.deployment_committed("old").await.unwrap();
        assert!(db.entity_cache.read().await.by_id.is_empty());
        db.deployment_committed("absent").await.unwrap();
        drop_visibility_fixture(db, schema).await;
    }

    fn ids(v: &[&str]) -> Arc<Vec<String>> {
        Arc::new(v.iter().map(|s| s.to_string()).collect())
    }

    #[tokio::test]
    async fn prefix_page_cold_path_is_one_statement_and_memoizes_ids() {
        use super::*;
        let Some((db, schema)) = visibility_fixture().await else {
            return;
        };
        let prefix = "urn:decentraland:matic:collections-v2:0xaaa";
        sqlx::query(
            "INSERT INTO deployments (id, entity_id, entity_type, entity_pointers, entity_metadata, deleter_deployment)
             VALUES (1, 'w1', 'wearable', ARRAY[$1 || ':1'], '{\"v\":{\"name\":\"one\"}}', NULL),
                    (2, 'w2', 'wearable', ARRAY[$1 || ':2'], NULL, NULL),
                    (3, 'w3', 'wearable', ARRAY[$1 || ':3'], NULL, NULL),
                    (4, 'other', 'wearable', ARRAY['urn:decentraland:matic:collections-v2:0xbbb:1'], NULL, NULL),
                    (5, 'gone', 'wearable', ARRAY[$1 || ':4'], NULL, 6)",
        )
        .bind(prefix)
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO active_pointers (pointer, entity_id)
             VALUES ($1 || ':1', 'w1'), ($1 || ':2', 'w2'), ($1 || ':3', 'w3'), ($1 || ':4', 'gone'),
                    ('urn:decentraland:matic:collections-v2:0xbbb:1', 'other')",
        )
        .bind(prefix)
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO content_files (deployment, key, content_hash) VALUES (1, 'b.png', 'hb'), (1, 'a.png', 'ha')")
            .execute(&db.pool)
            .await
            .unwrap();

        let cold = db.active_entity_docs_by_prefix(prefix, 0, 2).await.unwrap();
        assert_eq!(cold.total, 4);
        let memo = db.prefix_ids_cache.lock().await.get(prefix).unwrap();
        assert_eq!(memo.len(), 4);
        let page_ids: Vec<&str> = cold.entities.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(page_ids, &memo[..2]);
        let by_ids = db.active_entities_by_ids(&memo[..2]).await.unwrap();
        for doc in &cold.entities {
            let value = doc.to_value().unwrap();
            assert!(
                by_ids.contains(&value),
                "page row must match the by-id shape: {value}"
            );
        }
        let w1 = cold
            .entities
            .iter()
            .find(|d| d.id == "w1")
            .map(|d| d.to_value().unwrap());
        if let Some(w1) = w1 {
            assert_eq!(w1["metadata"]["name"], "one");
            assert_eq!(w1["content"][0]["file"], "a.png");
        }

        let warm = db
            .active_entity_docs_by_prefix(prefix, 2, 10)
            .await
            .unwrap();
        assert_eq!(warm.total, 4);
        let warm_ids: Vec<&str> = warm.entities.iter().map(|d| d.id.as_str()).collect();
        let expected: Vec<&str> = memo[2..]
            .iter()
            .map(String::as_str)
            .filter(|id| *id != "gone")
            .collect();
        assert_eq!(warm_ids, expected);

        let empty = db
            .active_entity_docs_by_prefix("urn:decentraland:matic:collections-v2:0x_none", 0, 10)
            .await
            .unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.entities.is_empty());
        assert_eq!(
            db.prefix_ids_cache
                .lock()
                .await
                .get("urn:decentraland:matic:collections-v2:0x_none")
                .unwrap()
                .len(),
            0
        );
        drop_visibility_fixture(db, schema).await;
    }

    #[tokio::test]
    async fn side_cache_fills_are_dropped_when_a_deployment_landed_in_between() {
        use super::*;
        let db = LiveDatabase {
            pool: PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap(),
            deployment_schema: catalyrst_server::schema_migrations::DeploymentSchema {
                local_entities: false,
                pointer_entity_type: false,
            },
            entity_cache: Arc::new(RwLock::new(EntityCache::new())),
            profile_lru: Arc::new(Mutex::new(ProfileLru::new(10))),
            prefix_ids_cache: Arc::new(Mutex::new(PrefixIdsCache::new(
                10,
                Duration::from_secs(60),
            ))),
        };
        let doc = EntityDoc {
            id: "p".into(),
            json: Bytes::from_static(b"{\"id\":\"p\"}"),
        };
        let generation = db.entity_cache.read().await.generation;
        db.entity_cache.write().await.generation += 1;
        db.remember_profiles(generation, vec![doc.clone()]).await;
        assert!(db.profile_lru.lock().await.get("p").is_none());
        let generation = db.entity_cache.read().await.generation;
        db.remember_profiles(generation, vec![doc]).await;
        assert!(db.profile_lru.lock().await.get("p").is_some());
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

    async fn migrated_fixture() -> Option<(super::LiveDatabase, String)> {
        use super::*;
        let url = catalyrst_testgate::require_pg("CATALYRST_SERVER_TEST_PG")?;
        let schema = format!("test_live_db_{}", uuid::Uuid::new_v4().simple());
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
        for sql in [
            include_str!("../../../migrations/0001_content_schema.sql"),
            include_str!("../../../migrations/0003_local_provenance.sql"),
            include_str!("../../../migrations/0005_active_pointers_entity_type.sql"),
            include_str!("../../../migrations/0006_failed_deployments_retry_backoff.sql"),
        ] {
            let body: String = sql
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .map(|l| l.replace("public.", ""))
                .collect::<Vec<_>>()
                .join("\n");
            sqlx::raw_sql(sqlx::AssertSqlSafe(body))
                .execute(&pool)
                .await
                .unwrap();
        }
        let deployment_schema =
            catalyrst_server::schema_migrations::DeploymentSchema::detect(&pool)
                .await
                .unwrap();
        assert!(deployment_schema.local_entities && deployment_schema.pointer_entity_type);
        let db = LiveDatabase {
            pool,
            deployment_schema,
            entity_cache: Arc::new(RwLock::new(EntityCache::new())),
            profile_lru: Arc::new(Mutex::new(ProfileLru::new(10))),
            prefix_ids_cache: Arc::new(Mutex::new(PrefixIdsCache::new(
                10,
                Duration::from_secs(60),
            ))),
        };
        Some((db, schema))
    }

    async fn insert_scene(pool: &sqlx::PgPool, entity_id: &str, ts_ms: i64, pointer: &str) -> i32 {
        sqlx::query_scalar(
            "INSERT INTO deployments
                (deployer_address, version, entity_type, entity_id, entity_metadata,
                 entity_timestamp, entity_pointers, local_timestamp, auth_chain)
             VALUES ('0xdeployer', 'v3', 'scene', $1, NULL, to_timestamp($2 / 1000.0),
                     ARRAY[$3::text], to_timestamp($2 / 1000.0), '[]'::json)
             RETURNING id",
        )
        .bind(entity_id)
        .bind(ts_ms as f64)
        .bind(pointer)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn deployments_page_carries_content_off_the_page_query() {
        use super::*;
        let Some((db, schema)) = migrated_fixture().await else {
            return;
        };
        let a = insert_scene(&db.pool, "bafya", 1_000, "0,0").await;
        insert_scene(&db.pool, "bafyb", 2_000, "1,1").await;
        sqlx::query(
            "INSERT INTO content_files (deployment, content_hash, key)
             VALUES ($1, 'hz', 'z.glb'), ($1, 'ha', 'a.png')",
        )
        .bind(a)
        .execute(&db.pool)
        .await
        .unwrap();

        let opts = DeploymentQueryOptions {
            fields: vec!["content".to_string()],
            sorting_field: Some("entity_timestamp".to_string()),
            sorting_order: Some("ASC".to_string()),
            ..Default::default()
        };
        let page = db.get_deployments(&opts).await.unwrap();
        let ids: Vec<&str> = page
            .deployments
            .iter()
            .map(|d| d.entity_id.as_str())
            .collect();
        assert_eq!(ids, vec!["bafya", "bafyb"]);
        let content: Vec<(String, String)> = page.deployments[0]
            .content
            .clone()
            .unwrap()
            .into_iter()
            .map(|c| (c.key, c.hash))
            .collect();
        assert_eq!(
            content,
            vec![
                ("a.png".to_string(), "ha".to_string()),
                ("z.glb".to_string(), "hz".to_string())
            ]
        );
        assert!(page.deployments[1].content.as_ref().unwrap().is_empty());

        let without = DeploymentQueryOptions {
            fields: vec![],
            ..opts
        };
        let page = db.get_deployments(&without).await.unwrap();
        assert!(page.deployments[0].content.as_ref().unwrap().is_empty());
        drop_visibility_fixture(db, schema).await;
    }

    #[tokio::test]
    async fn audit_info_joins_local_provenance_in_one_statement() {
        use super::*;
        let Some((db, schema)) = migrated_fixture().await else {
            return;
        };
        insert_scene(&db.pool, "bafylocal", 1_000, "0,0").await;
        insert_scene(&db.pool, "bafysynced", 2_000, "1,1").await;
        sqlx::query(
            "INSERT INTO local_entities (entity_id, signer) VALUES ('bafylocal', '0xOwner')",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        let local = db
            .get_audit_info("scene", "bafylocal")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(local["version"], "v3");
        assert_eq!(local["localTimestamp"], 1_000);
        assert!(local["authChain"].is_array());
        assert_eq!(local["localProvenance"]["signer"], "0xOwner");
        assert_eq!(local["localProvenance"]["origin"], "land-publish");
        assert_eq!(local["localProvenance"]["status"], "active");
        assert_eq!(local["localProvenance"]["superseded"], false);
        assert!(local["localProvenance"]["publishedAt"].is_i64());

        let synced = db
            .get_audit_info("scene", "bafysynced")
            .await
            .unwrap()
            .unwrap();
        assert!(synced.get("localProvenance").is_none());
        assert!(db
            .get_audit_info("scene", "missing")
            .await
            .unwrap()
            .is_none());
        drop_visibility_fixture(db, schema).await;
    }

    #[tokio::test]
    async fn failed_deployments_page_is_windowed_in_sql() {
        use super::*;
        let Some((db, schema)) = migrated_fixture().await else {
            return;
        };
        for (id, ts) in [("f1", 1_000), ("f2", 2_000), ("f3", 3_000)] {
            sqlx::query(
                "INSERT INTO failed_deployments
                    (entity_id, entity_type, failure_time, reason, auth_chain, error_description, snapshot_hash)
                 VALUES ($1, 'scene', to_timestamp($2 / 1000.0), 'DEPLOYMENT_ERROR', '[]'::json, 'boom', 'snap')",
            )
            .bind(id)
            .bind(ts as f64)
            .execute(&db.pool)
            .await
            .unwrap();
        }
        let all = db.get_failed_deployments().await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0]["entityId"], "f1");
        assert_eq!(all[0]["failureTimestamp"], 1_000);
        assert_eq!(all[0]["retryCount"], 0);
        assert!(all[0]["nextRetryAt"].is_i64());

        let page = db.get_failed_deployments_page(1, 1).await.unwrap();
        assert_eq!(page, vec![all[1].clone()]);
        assert_eq!(db.get_failed_deployments_page(0, 10).await.unwrap(), all);
        assert!(db
            .get_failed_deployments_page(3, 10)
            .await
            .unwrap()
            .is_empty());
        assert!(db
            .get_failed_deployments_page(0, 0)
            .await
            .unwrap()
            .is_empty());
        drop_visibility_fixture(db, schema).await;
    }
}
