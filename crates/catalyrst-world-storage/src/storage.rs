use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::config::NamespaceLimits;
use crate::http::errors::ApiError;

#[derive(Debug)]
pub struct StorageEntry {
    pub key: String,
    pub value: Value,
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
    prefix.filter(|p| !p.is_empty()).map(|p| format!("{p}%"))
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

#[derive(Clone)]
pub struct Storage {
    pub pool: PgPool,
}

impl Storage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn world_get(
        &self,
        world: &str,
        place: &str,
        key: &str,
    ) -> Result<Option<Value>, ApiError> {
        let row = sqlx::query(
            "SELECT value FROM world_storage WHERE world_name = $1 AND place_id = $2::uuid AND key = $3",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<Value, _>("value")))
    }

    pub async fn world_set(
        &self,
        world: &str,
        place: &str,
        key: &str,
        value: &Value,
        size: i64,
    ) -> Result<Value, ApiError> {
        let row = sqlx::query(
            "INSERT INTO world_storage (world_name, place_id, key, value, value_size, created_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, now(), now())
             ON CONFLICT (world_name, place_id, key)
             DO UPDATE SET value = $4, value_size = $5, updated_at = now()
             RETURNING value",
        )
        .bind(world)
        .bind(place)
        .bind(key)
        .bind(value)
        .bind(size)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<Value, _>("value"))
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
        Ok(())
    }

    pub async fn world_delete_all(&self, world: &str, place: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM world_storage WHERE world_name = $1 AND place_id = $2::uuid")
            .bind(world)
            .bind(place)
            .execute(&self.pool)
            .await?;
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
            "SELECT key, value FROM world_storage
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
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(value_size) FILTER (WHERE key = $2), 0)::bigint AS existing,
                    COALESCE(SUM(value_size), 0)::bigint AS total
             FROM world_storage WHERE world_name = $1",
        )
        .bind(world)
        .bind(key)
        .fetch_one(&self.pool)
        .await?;
        Ok(SizeInfo {
            existing_value_size: row.get("existing"),
            total_size: row.get("total"),
        })
    }

    pub async fn player_get(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: &str,
    ) -> Result<Option<Value>, ApiError> {
        let row = sqlx::query(
            "SELECT value FROM player_storage
             WHERE world_name = $1 AND place_id = $2::uuid AND player_address = $3 AND key = $4",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<Value, _>("value")))
    }

    pub async fn player_set(
        &self,
        world: &str,
        place: &str,
        player: &str,
        key: &str,
        value: &Value,
        size: i64,
    ) -> Result<Value, ApiError> {
        let row = sqlx::query(
            "INSERT INTO player_storage (world_name, place_id, player_address, key, value, value_size, created_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, $6, now(), now())
             ON CONFLICT (world_name, place_id, player_address, key)
             DO UPDATE SET value = $5, value_size = $6, updated_at = now()
             RETURNING value",
        )
        .bind(world)
        .bind(place)
        .bind(player)
        .bind(key)
        .bind(value)
        .bind(size)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<Value, _>("value"))
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
        Ok(())
    }

    pub async fn player_delete_all(&self, world: &str, place: &str) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM player_storage WHERE world_name = $1 AND place_id = $2::uuid")
            .bind(world)
            .bind(place)
            .execute(&self.pool)
            .await?;
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
            "SELECT key, value FROM player_storage
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
        player: &str,
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(value_size) FILTER (WHERE key = $3), 0)::bigint AS existing,
                    COALESCE(SUM(value_size), 0)::bigint AS total
             FROM player_storage WHERE world_name = $1 AND player_address = $2",
        )
        .bind(world)
        .bind(player)
        .bind(key)
        .fetch_one(&self.pool)
        .await?;
        Ok(SizeInfo {
            existing_value_size: row.get("existing"),
            total_size: row.get("total"),
        })
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

    pub async fn env_set(
        &self,
        world: &str,
        place: &str,
        key: &str,
        value_enc: &[u8],
        size: i64,
    ) -> Result<(), ApiError> {
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
        .execute(&self.pool)
        .await?;
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
        key: Option<&str>,
    ) -> Result<SizeInfo, ApiError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(value_size) FILTER (WHERE key = $2), 0)::bigint AS existing,
                    COALESCE(SUM(value_size), 0)::bigint AS total
             FROM env_variables WHERE world_name = $1",
        )
        .bind(world)
        .bind(key)
        .fetch_one(&self.pool)
        .await?;
        Ok(SizeInfo {
            existing_value_size: row.get("existing"),
            total_size: row.get("total"),
        })
    }
}
