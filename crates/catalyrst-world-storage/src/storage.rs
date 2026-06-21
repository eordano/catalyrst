use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use sqlx::{PgExecutor, PgPool, Row};

use crate::config::{NamespaceLimits, StorageCacheConfig};
use crate::external::is_shared_realm;
use crate::http::errors::ApiError;

#[derive(Debug)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy)]
pub struct SizeInfo {
    pub existing_value_size: i64,
    pub total_size: i64,
}

pub fn value_size_bytes(serialized: &str) -> i64 {
    serialized.len() as i64
}

fn prefix_pattern(prefix: Option<&str>) -> Option<String> {
    prefix.filter(|p| !p.is_empty()).map(|p| {
        let mut pat = String::with_capacity(p.len() + 1);
        for c in p.chars() {
            if matches!(c, '\\' | '%' | '_') {
                pat.push('\\');
            }
            pat.push(c);
        }
        pat.push('%');
        pat
    })
}

fn quota_lock_key(namespace: &str, world: &str, place: &str, player: Option<&str>) -> String {
    let mut key = if is_shared_realm(world) {
        format!("{namespace}:{world}:{place}")
    } else {
        format!("{namespace}:{world}")
    };
    if let Some(player) = player {
        key.push(':');
        key.push_str(player);
    }
    key
}

fn size_info_sql(table: &str, world: &str, extra_where: &str) -> String {
    let mut sql = format!(
        "SELECT COALESCE(MAX(value_size) FILTER (WHERE place_id = $2::uuid AND key = $3), 0)::bigint AS existing,
                COALESCE(SUM(value_size), 0)::bigint AS total
         FROM {table} WHERE world_name = $1{extra_where}"
    );
    if is_shared_realm(world) {
        sql.push_str(" AND place_id = $2::uuid");
    }
    sql
}

pub fn check_limits(
    new_value_size: i64,
    info: SizeInfo,
    limits: NamespaceLimits,
) -> Result<(), ApiError> {
    if new_value_size > limits.max_value_size_bytes {
        return Err(ApiError::bad_request(format!(
            "Value size ({} bytes) exceeds the maximum allowed size ({} bytes)",
            new_value_size, limits.max_value_size_bytes
        )));
    }
    let projected = info.total_size - info.existing_value_size + new_value_size;
    if projected > limits.max_total_size_bytes {
        return Err(ApiError::bad_request(format!(
            "Total storage size would exceed the maximum allowed ({} bytes). Current usage: {} bytes. Delete existing data to free up space",
            limits.max_total_size_bytes, info.total_size
        )));
    }
    Ok(())
}

fn world_value_cache_key(world: &str, place: &str, key: &str) -> String {
    format!("w:{world}:{place}:{key}")
}

fn world_scene_prefix(world: &str, place: &str) -> String {
    format!("w:{world}:{place}:")
}

fn player_value_cache_key(world: &str, place: &str, player: &str, key: &str) -> String {
    format!("p:{world}:{place}:{player}:{key}")
}

fn player_scope_prefix(world: &str, place: &str, player: Option<&str>) -> String {
    match player {
        Some(p) => format!("p:{world}:{place}:{p}:"),
        None => format!("p:{world}:{place}:"),
    }
}

#[derive(Clone)]
pub struct StorageCache {
    cfg: StorageCacheConfig,
    cache: Cache<String, Arc<str>>,
}

impl StorageCache {
    pub fn new(cfg: StorageCacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(cfg.max_entries)
            .time_to_live(Duration::from_secs(cfg.ttl_seconds))
            .support_invalidation_closures()
            .build();
        Self { cfg, cache }
    }

    async fn get(&self, key: &str) -> Option<Arc<str>> {
        if !self.cfg.enabled {
            return None;
        }
        self.cache.get(key).await
    }

    async fn insert(&self, key: String, value: Arc<str>) {
        if !self.cfg.enabled || value.len() > self.cfg.max_value_bytes {
            return;
        }
        self.cache.insert(key, value).await;
    }

    async fn invalidate(&self, key: &str) {
        if !self.cfg.enabled {
            return;
        }
        self.cache.invalidate(key).await;
    }

    fn invalidate_prefix(&self, prefix: String) {
        if !self.cfg.enabled {
            return;
        }
        self.cache
            .invalidate_entries_if(move |k, _| k.starts_with(&prefix))
            .expect("invalidation closures enabled at build");
    }
}

#[derive(Clone)]
pub struct Storage {
    pub pool: PgPool,
    cache: StorageCache,
}

impl Storage {
    pub fn new(pool: PgPool, cache_cfg: StorageCacheConfig) -> Self {
        Self {
            pool,
            cache: StorageCache::new(cache_cfg),
        }
    }

    pub async fn world_get(
        &self,
        world: &str,
        place: &str,
        key: &str,
    ) -> Result<Option<Arc<str>>, ApiError> {
        let cache_key = world_value_cache_key(world, place, key);
        if let Some(hit) = self.cache.get(&cache_key).await {
            return Ok(Some(hit));
        }
        let row = sqlx::query(
            "SELECT value::text AS value FROM world_storage WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let value: Arc<str> = Arc::from(row.get::<String, _>("value"));
        self.cache.insert(cache_key, value.clone()).await;
        Ok(Some(value))
    }

    pub async fn world_upsert_with_quota(
        &self,
        world: &str,
        place: &str,
        key: &str,
        serialized: &str,
        limits: NamespaceLimits,
    ) -> Result<(), ApiError> {
        let size = value_size_bytes(serialized);
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(quota_lock_key("world-storage", world, place, None))
            .execute(&mut *tx)
            .await?;
        let info = world_size_info_in(&mut *tx, world, place, Some(key)).await?;
        check_limits(size, info, limits)?;
        sqlx::query(
            "INSERT INTO world_storage (world_name, place_id, key, value, value_size, created_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4::jsonb, $5, now(), now())
             ON CONFLICT (world_name, place_id, key)
             DO UPDATE SET value = $4::jsonb, value_size = $5, updated_at = now()",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .bind(serialized)
        .bind(size)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.cache
            .invalidate(&world_value_cache_key(world, place, key))
            .await;
        Ok(())
    }

    pub async fn world_delete(&self, world: &str, place: &str, key: &str) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM world_storage WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .execute(&self.pool)
        .await?;
        self.cache
            .invalidate(&world_value_cache_key(world, place, key))
            .await;
        Ok(())
    }

    pub async fn world_delete_all(&self, world: &str, place: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM world_storage WHERE world_name = $1 AND place_id = $2::uuid")
            .bind(world)
            .bind(place)
            .execute(&self.pool)
            .await?;
        self.cache
            .invalidate_prefix(world_scene_prefix(world, place));
        Ok(())
    }

    pub async fn world_list(
        &self,
        world: &str,
        place: &str,
        limit: i64,
        offset: i64,
        prefix: Option<&str>,
    ) -> Result<Vec<StorageEntry>, ApiError> {
        let pat = prefix_pattern(prefix);
        let rows = sqlx::query(
            "SELECT key, value::text AS value FROM world_storage
             WHERE world_name = $1 AND place_id = $2::uuid
               AND ($3::text IS NULL OR key LIKE $3)
             ORDER BY key ASC LIMIT $4 OFFSET $5",
        )
        .bind(world)
        .bind(place)
        .bind(pat)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| StorageEntry {
                key: r.get("key"),
                value: r.get("value"),
            })
            .collect())
    }

    pub async fn world_count(
        &self,
        world: &str,
        place: &str,
        prefix: Option<&str>,
    ) -> Result<i64, ApiError> {
        let pat = prefix_pattern(prefix);
        let row = sqlx::query(
            "SELECT COUNT(*)::bigint AS count FROM world_storage
             WHERE world_name = $1 AND place_id = $2::uuid
               AND ($3::text IS NULL OR key LIKE $3)",
        )
        .bind(world)
        .bind(place)
        .bind(pat)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn world_size_info(
        &self,
        world: &str,
        place: &str,
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        world_size_info_in(&self.pool, world, place, key).await
    }

    pub async fn player_get(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: &str,
    ) -> Result<Option<Arc<str>>, ApiError> {
        let cache_key = player_value_cache_key(world, place, player, key);
        if let Some(hit) = self.cache.get(&cache_key).await {
            return Ok(Some(hit));
        }
        let row = sqlx::query(
            "SELECT value::text AS value FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3 AND key = $4",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let value: Arc<str> = Arc::from(row.get::<String, _>("value"));
        self.cache.insert(cache_key, value.clone()).await;
        Ok(Some(value))
    }

    pub async fn player_upsert_with_quota(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: &str,
        serialized: &str,
        limits: NamespaceLimits,
    ) -> Result<(), ApiError> {
        let size = value_size_bytes(serialized);
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(quota_lock_key("player-storage", world, place, Some(player)))
            .execute(&mut *tx)
            .await?;
        let info = player_size_info_in(&mut *tx, world, place, player, Some(key)).await?;
        check_limits(size, info, limits)?;
        sqlx::query(
            "INSERT INTO player_storage (world_name, place_id, player_address, key, value, value_size, created_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5::jsonb, $6, now(), now())
             ON CONFLICT (world_name, place_id, player_address, key)
             DO UPDATE SET value = $5::jsonb, value_size = $6, updated_at = now()",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(key)
        .bind(serialized)
        .bind(size)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.cache
            .invalidate(&player_value_cache_key(world, place, player, key))
            .await;
        Ok(())
    }

    pub async fn player_delete(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3 AND key = $4",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(key)
        .execute(&self.pool)
        .await?;
        self.cache
            .invalidate(&player_value_cache_key(world, place, player, key))
            .await;
        Ok(())
    }

    pub async fn player_delete_all_for_player(
        &self,
        world: &str,
        place: &str,
        player: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .execute(&self.pool)
        .await?;
        self.cache
            .invalidate_prefix(player_scope_prefix(world, place, Some(player)));
        Ok(())
    }

    pub async fn player_delete_all(&self, world: &str, place: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM player_storage WHERE world_name = $1 AND place_id = $2::uuid")
            .bind(world)
            .bind(place)
            .execute(&self.pool)
            .await?;
        self.cache
            .invalidate_prefix(player_scope_prefix(world, place, None));
        Ok(())
    }

    pub async fn player_list(
        &self,
        world: &str,
        place: &str,
        player: &str,
        limit: i64,
        offset: i64,
        prefix: Option<&str>,
    ) -> Result<Vec<StorageEntry>, ApiError> {
        let pat = prefix_pattern(prefix);
        let rows = sqlx::query(
            "SELECT key, value::text AS value FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3
               AND ($4::text IS NULL OR key LIKE $4)
             ORDER BY key ASC LIMIT $5 OFFSET $6",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(pat)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| StorageEntry {
                key: r.get("key"),
                value: r.get("value"),
            })
            .collect())
    }

    pub async fn player_count(
        &self,
        world: &str,
        place: &str,
        player: &str,
        prefix: Option<&str>,
    ) -> Result<i64, ApiError> {
        let pat = prefix_pattern(prefix);
        let row = sqlx::query(
            "SELECT COUNT(*)::bigint AS count FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3
               AND ($4::text IS NULL OR key LIKE $4)",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(pat)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn player_list_players(
        &self,
        world: &str,
        place: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, ApiError> {
        let rows = sqlx::query(
            "SELECT DISTINCT player_address FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid
             ORDER BY player_address ASC LIMIT $3 OFFSET $4",
        )
        .bind(world)
        .bind(place)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get("player_address")).collect())
    }

    pub async fn player_count_players(&self, world: &str, place: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            "SELECT COUNT(DISTINCT player_address)::bigint AS count FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid",
        )
        .bind(world)
        .bind(place)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn player_size_info(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        player_size_info_in(&self.pool, world, place, player, key).await
    }

    pub async fn env_get_enc(
        &self,
        world: &str,
        place: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, ApiError> {
        let row = sqlx::query(
            "SELECT value_enc FROM env_variables WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<Vec<u8>, _>("value_enc")))
    }

    pub async fn env_upsert_with_quota(
        &self,
        world: &str,
        place: &str,
        key: &str,
        value_enc: &[u8],
        size: i64,
        limits: NamespaceLimits,
    ) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(quota_lock_key("env-storage", world, place, None))
            .execute(&mut *tx)
            .await?;
        let info = env_size_info_in(&mut *tx, world, place, Some(key)).await?;
        check_limits(size, info, limits)?;
        sqlx::query(
            "INSERT INTO env_variables (world_name, place_id, key, value_enc, value_size, created_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, now(), now())
             ON CONFLICT (world_name, place_id, key)
             DO UPDATE SET value_enc = $4, value_size = $5, updated_at = now()",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .bind(value_enc)
        .bind(size)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn env_delete(&self, world: &str, place: &str, key: &str) -> Result<(), ApiError> {
        sqlx::query(
            "DELETE FROM env_variables WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn env_delete_all(&self, world: &str, place: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM env_variables WHERE world_name = $1 AND place_id = $2::uuid")
            .bind(world)
            .bind(place)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn env_list_keys(
        &self,
        world: &str,
        place: &str,
        limit: i64,
        offset: i64,
        prefix: Option<&str>,
    ) -> Result<Vec<String>, ApiError> {
        let pat = prefix_pattern(prefix);
        let rows = sqlx::query(
            "SELECT key FROM env_variables
             WHERE world_name = $1 AND place_id = $2::uuid
               AND ($3::text IS NULL OR key LIKE $3)
             ORDER BY key ASC LIMIT $4 OFFSET $5",
        )
        .bind(world)
        .bind(place)
        .bind(pat)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get("key")).collect())
    }

    pub async fn env_count(
        &self,
        world: &str,
        place: &str,
        prefix: Option<&str>,
    ) -> Result<i64, ApiError> {
        let pat = prefix_pattern(prefix);
        let row = sqlx::query(
            "SELECT COUNT(*)::bigint AS count FROM env_variables
             WHERE world_name = $1 AND place_id = $2::uuid
               AND ($3::text IS NULL OR key LIKE $3)",
        )
        .bind(world)
        .bind(place)
        .bind(pat)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("count"))
    }

    pub async fn env_size_info(
        &self,
        world: &str,
        place: &str,
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        env_size_info_in(&self.pool, world, place, key).await
    }
}

async fn world_size_info_in<'e, E: PgExecutor<'e>>(
    exec: E,
    world: &str,
    place: &str,
    key: Option<&str>,
) -> Result<SizeInfo, ApiError> {
    let sql = size_info_sql("world_storage", world, "");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(world)
        .bind(place)
        .bind(key)
        .fetch_one(exec)
        .await?;
    Ok(SizeInfo {
        existing_value_size: row.get("existing"),
        total_size: row.get("total"),
    })
}

async fn player_size_info_in<'e, E: PgExecutor<'e>>(
    exec: E,
    world: &str,
    place: &str,
    player: &str,
    key: Option<&str>,
) -> Result<SizeInfo, ApiError> {
    let sql = size_info_sql("player_storage", world, " AND player_address = $4");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(world)
        .bind(place)
        .bind(key)
        .bind(player)
        .fetch_one(exec)
        .await?;
    Ok(SizeInfo {
        existing_value_size: row.get("existing"),
        total_size: row.get("total"),
    })
}

async fn env_size_info_in<'e, E: PgExecutor<'e>>(
    exec: E,
    world: &str,
    place: &str,
    key: Option<&str>,
) -> Result<SizeInfo, ApiError> {
    let sql = size_info_sql("env_variables", world, "");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(world)
        .bind(place)
        .bind(key)
        .fetch_one(exec)
        .await?;
    Ok(SizeInfo {
        existing_value_size: row.get("existing"),
        total_size: row.get("total"),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        player_scope_prefix, player_value_cache_key, prefix_pattern, quota_lock_key, size_info_sql,
        world_scene_prefix, world_value_cache_key, StorageCache,
    };
    use crate::config::StorageCacheConfig;

    fn cache_cfg(enabled: bool) -> StorageCacheConfig {
        StorageCacheConfig {
            enabled,
            ttl_seconds: 60,
            max_entries: 100,
            max_value_bytes: 64,
        }
    }

    #[test]
    fn cache_keys_scope_by_namespace_scene_and_player() {
        let w = world_value_cache_key("foo.dcl.eth", "p1", "k");
        assert_eq!(w, "w:foo.dcl.eth:p1:k");
        assert!(w.starts_with(&world_scene_prefix("foo.dcl.eth", "p1")));

        let p = player_value_cache_key("foo.dcl.eth", "p1", "0xabc", "k");
        assert_eq!(p, "p:foo.dcl.eth:p1:0xabc:k");
        assert!(p.starts_with(&player_scope_prefix("foo.dcl.eth", "p1", Some("0xabc"))));
        assert!(p.starts_with(&player_scope_prefix("foo.dcl.eth", "p1", None)));
        assert!(!p.starts_with(&world_scene_prefix("foo.dcl.eth", "p1")));
        assert!(!player_value_cache_key("foo.dcl.eth", "p1", "0xabcd", "k")
            .starts_with(&player_scope_prefix("foo.dcl.eth", "p1", Some("0xabc"))));
    }

    #[tokio::test]
    async fn cache_hits_after_insert_and_misses_after_invalidate() {
        let cache = StorageCache::new(cache_cfg(true));
        let key = world_value_cache_key("w.dcl.eth", "p1", "k");
        assert_eq!(cache.get(&key).await, None);

        cache.insert(key.clone(), Arc::from("{\"a\":1}")).await;
        assert_eq!(cache.get(&key).await.as_deref(), Some("{\"a\":1}"));

        cache.invalidate(&key).await;
        assert_eq!(cache.get(&key).await, None);
    }

    #[tokio::test]
    async fn prefix_invalidation_clears_the_scope_and_spares_others() {
        let cache = StorageCache::new(cache_cfg(true));
        let mine = player_value_cache_key("w.dcl.eth", "p1", "0xabc", "k");
        let other_player = player_value_cache_key("w.dcl.eth", "p1", "0xdef", "k");
        let other_scene = player_value_cache_key("w.dcl.eth", "p2", "0xabc", "k");
        for k in [&mine, &other_player, &other_scene] {
            cache.insert(k.clone(), Arc::from("1")).await;
        }

        cache.invalidate_prefix(player_scope_prefix("w.dcl.eth", "p1", Some("0xabc")));
        assert_eq!(cache.get(&mine).await, None);
        assert!(cache.get(&other_player).await.is_some());
        assert!(cache.get(&other_scene).await.is_some());

        cache.invalidate_prefix(player_scope_prefix("w.dcl.eth", "p1", None));
        assert_eq!(cache.get(&other_player).await, None);
        assert!(cache.get(&other_scene).await.is_some());
    }

    #[tokio::test]
    async fn disabled_cache_and_oversized_values_are_never_stored() {
        let cache = StorageCache::new(cache_cfg(false));
        cache.insert("k".to_string(), Arc::from("1")).await;
        assert_eq!(cache.get("k").await, None);

        let cache = StorageCache::new(cache_cfg(true));
        let oversized = "x".repeat(65);
        cache
            .insert(
                "big".to_string(),
                Arc::from(format!("\"{oversized}\"").as_str()),
            )
            .await;
        assert_eq!(cache.get("big").await, None);
    }

    #[test]
    fn prefix_pattern_escapes_like_wildcards() {
        assert_eq!(prefix_pattern(None), None);
        assert_eq!(prefix_pattern(Some("")), None);
        assert_eq!(prefix_pattern(Some("abc")).as_deref(), Some("abc%"));
        assert_eq!(prefix_pattern(Some("a%b")).as_deref(), Some("a\\%b%"));
        assert_eq!(prefix_pattern(Some("a_b")).as_deref(), Some("a\\_b%"));
        assert_eq!(prefix_pattern(Some("a\\b")).as_deref(), Some("a\\\\b%"));
    }

    #[test]
    fn quota_lock_scope_is_world_place_or_player() {
        assert_eq!(
            quota_lock_key("world-storage", "foo.dcl.eth", "p1", None),
            "world-storage:foo.dcl.eth"
        );
        assert_eq!(
            quota_lock_key("world-storage", "main", "p1", None),
            "world-storage:main:p1"
        );
        assert_eq!(
            quota_lock_key("player-storage", "foo.dcl.eth", "p1", Some("0xabc")),
            "player-storage:foo.dcl.eth:0xabc"
        );
        assert_eq!(
            quota_lock_key("player-storage", "main", "p1", Some("0xabc")),
            "player-storage:main:p1:0xabc"
        );
    }

    #[test]
    fn size_query_credits_the_exact_place_and_scopes_shared_realms_per_place() {
        let world = size_info_sql("world_storage", "foo.dcl.eth", "");
        assert!(world.contains("FILTER (WHERE place_id = $2::uuid AND key = $3)"));
        assert!(!world.trim_end().ends_with("AND place_id = $2::uuid"));

        let shared = size_info_sql("world_storage", "main", "");
        assert!(shared.trim_end().ends_with("AND place_id = $2::uuid"));

        let player = size_info_sql("player_storage", "main", " AND player_address = $4");
        assert!(player.contains("AND player_address = $4 AND place_id = $2::uuid"));
    }
}
