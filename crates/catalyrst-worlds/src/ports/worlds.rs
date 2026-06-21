use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::access::AccessSetting;
use crate::http::ApiError;

#[derive(Debug, Clone)]
pub struct WorldRecord {
    pub name: String,
    pub owner: Option<String>,
    pub access: AccessSetting,
    pub blocked_since: Option<DateTime<Utc>>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub single_player: bool,
}

#[derive(Debug, Clone)]
pub struct WorldScene {
    pub entity_id: String,
    pub entity: Value,
    pub parcels: Vec<String>,
}

#[derive(Clone)]
pub struct WorldsComponent {
    pool: PgPool,
}

impl WorldsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn get_world(&self, world_name: &str) -> Result<Option<WorldRecord>, ApiError> {
        let row = sqlx::query(
            r#"SELECT name, owner, access, blocked_since, spawn_coordinates,
                      skybox_time, single_player
               FROM worlds WHERE lower(name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let access = r
                .get::<Option<Value>, _>("access")
                .and_then(|v| serde_json::from_value::<AccessSetting>(v).ok())
                .unwrap_or_default();
            WorldRecord {
                name: r.get("name"),
                owner: r.get("owner"),
                access,
                blocked_since: r.get("blocked_since"),
                spawn_coordinates: r.get("spawn_coordinates"),
                skybox_time: r.get("skybox_time"),
                single_player: r.get::<Option<bool>, _>("single_player").unwrap_or(false),
            }
        }))
    }

    pub async fn get_access(&self, world_name: &str) -> Result<AccessSetting, ApiError> {
        Ok(self
            .get_world(world_name)
            .await?
            .map(|w| w.access)
            .unwrap_or_default())
    }

    pub async fn is_world_valid(&self, world_name: &str) -> Result<bool, ApiError> {
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM world_scenes WHERE lower(world_name) = lower($1)
               )"#,
        )
        .bind(world_name)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    pub async fn get_scenes(&self, world_name: &str) -> Result<Vec<WorldScene>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT entity_id, entity, parcels
               FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY created_at DESC"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| WorldScene {
                entity_id: r.get("entity_id"),
                entity: r.get("entity"),
                parcels: r.get("parcels"),
            })
            .collect())
    }

    pub async fn list_index_scenes(&self) -> Result<Vec<(String, WorldScene)>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT ws.world_name, ws.entity_id, ws.entity, ws.parcels
               FROM world_scenes ws
               JOIN worlds w ON lower(w.name) = lower(ws.world_name)
               WHERE w.blocked_since IS NULL
               ORDER BY ws.world_name, ws.created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let world_name: String = r.get("world_name");
                (
                    world_name,
                    WorldScene {
                        entity_id: r.get("entity_id"),
                        entity: r.get("entity"),
                        parcels: r.get("parcels"),
                    },
                )
            })
            .collect())
    }

    pub async fn get_entities_for_worlds(
        &self,
        world_names: &[String],
    ) -> Result<Vec<Value>, ApiError> {
        if world_names.is_empty() {
            return Ok(Vec::new());
        }

        let lowered: Vec<String> = world_names.iter().map(|w| w.to_lowercase()).collect();

        let rows = sqlx::query(
            r#"SELECT DISTINCT ON (lower(ws.world_name))
                      ws.entity_id, ws.entity, w.owner
               FROM world_scenes ws
               JOIN worlds w ON lower(w.name) = lower(ws.world_name)
               WHERE lower(ws.world_name) = ANY($1)
               ORDER BY lower(ws.world_name), ws.created_at DESC"#,
        )
        .bind(&lowered)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let entity_id: String = r.get("entity_id");
                let owner: Option<String> = r.get("owner");
                let mut entity: Value = r.get("entity");
                if let Some(obj) = entity.as_object_mut() {
                    obj.insert("id".into(), Value::String(entity_id));
                    let metadata = obj
                        .entry("metadata")
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    if let (Some(meta_obj), Some(owner)) = (metadata.as_object_mut(), owner) {
                        meta_obj.insert("owner".into(), Value::String(owner));
                    }
                }
                entity
            })
            .collect())
    }

    pub async fn get_scene_base_parcel(
        &self,
        world_name: &str,
        scene_id: &str,
    ) -> Result<Option<String>, ApiError> {
        let row = sqlx::query(
            r#"SELECT entity, parcels
               FROM world_scenes
               WHERE lower(world_name) = lower($1) AND entity_id = $2"#,
        )
        .bind(world_name)
        .bind(scene_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| {
            let entity: Value = r.get("entity");
            let parcels: Vec<String> = r.get("parcels");
            let base = entity
                .get("metadata")
                .and_then(|m| m.get("scene"))
                .and_then(|s| s.get("base"))
                .and_then(|b| b.as_str())
                .map(|s| s.to_string());
            base.or_else(|| parcels.first().cloned())
        }))
    }

    pub async fn get_permission_records(
        &self,
        world_name: &str,
    ) -> Result<Vec<(String, String)>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT address, permission_type FROM world_permissions
               WHERE lower(world_name) = lower($1)
               ORDER BY address, permission_type"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("address"), r.get("permission_type")))
            .collect())
    }

    pub async fn is_wallet_blocked(&self, wallet: &str) -> Result<bool, ApiError> {
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM blocked WHERE lower(wallet) = lower($1)
               )"#,
        )
        .bind(wallet)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    // ----- admin queries / world-owned mutations (admin-console §4) -----

    /// List worlds for the admin view. Returns name, owner, access type tag,
    /// whether the world is blocked (over-storage marker), block timestamp, and
    /// the count of deployed scenes.
    pub async fn admin_list_worlds(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorldAdminRow>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT w.name,
                      w.owner,
                      w.access,
                      w.blocked_since,
                      w.spawn_coordinates,
                      (SELECT count(*) FROM world_scenes ws
                         WHERE lower(ws.world_name) = lower(w.name)) AS scene_count
               FROM worlds w
               ORDER BY w.name
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let access_type = r
                    .get::<Option<Value>, _>("access")
                    .and_then(|v| {
                        v.get("type")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "unrestricted".to_string());
                WorldAdminRow {
                    name: r.get("name"),
                    owner: r.get("owner"),
                    access_type,
                    blocked_since: r.get("blocked_since"),
                    spawn_coordinates: r.get("spawn_coordinates"),
                    scene_count: r.get("scene_count"),
                }
            })
            .collect())
    }

    pub async fn admin_count_worlds(&self) -> Result<i64, ApiError> {
        Ok(sqlx::query_scalar(r#"SELECT count(*) FROM worlds"#)
            .fetch_one(&self.pool)
            .await?)
    }

    /// Enable (`blocked = None`) or disable (`blocked = Some(now)`) a world by
    /// setting the over-storage `blocked_since` marker. Returns false if no such
    /// world exists.
    pub async fn admin_set_world_blocked(
        &self,
        world_name: &str,
        blocked: bool,
    ) -> Result<bool, ApiError> {
        let sql = if blocked {
            r#"UPDATE worlds SET blocked_since = now(), updated_at = now()
               WHERE lower(name) = lower($1)"#
        } else {
            r#"UPDATE worlds SET blocked_since = NULL, updated_at = now()
               WHERE lower(name) = lower($1)"#
        };
        let res = sqlx::query(sql)
            .bind(world_name)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// List the platform block list (wallets blocked from the realm).
    pub async fn admin_list_blocked(&self) -> Result<Vec<BlockedRow>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT wallet, created_at, updated_at FROM blocked ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| BlockedRow {
                wallet: r.get("wallet"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    /// Add a wallet to the platform block list (idempotent).
    pub async fn admin_block_wallet(&self, wallet: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"INSERT INTO blocked (wallet) VALUES (lower($1))
               ON CONFLICT (wallet) DO UPDATE SET updated_at = now()"#,
        )
        .bind(wallet)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remove a wallet from the platform block list. Returns false if absent.
    pub async fn admin_unblock_wallet(&self, wallet: &str) -> Result<bool, ApiError> {
        let res = sqlx::query(r#"DELETE FROM blocked WHERE lower(wallet) = lower($1)"#)
            .bind(wallet)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Record a world access event (join/leave) from the LiveKit webhook.
    pub async fn record_access(
        &self,
        world_name: &str,
        address: &str,
        action: &str,
        room: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"INSERT INTO world_access_log (world_name, address, action, room)
               VALUES ($1, lower($2), $3, $4)"#,
        )
        .bind(world_name)
        .bind(address)
        .bind(action)
        .bind(room)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Query the world access log, optionally filtered by world and/or address.
    pub async fn admin_query_access_log(
        &self,
        world_name: Option<&str>,
        address: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AccessLogRow>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT id, world_name, address, action, room, created_at
               FROM world_access_log
               WHERE ($1::text IS NULL OR lower(world_name) = lower($1))
                 AND ($2::text IS NULL OR lower(address) = lower($2))
               ORDER BY created_at DESC, id DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(world_name)
        .bind(address)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AccessLogRow {
                id: r.get("id"),
                world_name: r.get("world_name"),
                address: r.get("address"),
                action: r.get("action"),
                room: r.get("room"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct WorldAdminRow {
    pub name: String,
    pub owner: Option<String>,
    pub access_type: String,
    pub blocked_since: Option<DateTime<Utc>>,
    pub spawn_coordinates: Option<String>,
    pub scene_count: i64,
}

#[derive(Debug, Clone)]
pub struct BlockedRow {
    pub wallet: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AccessLogRow {
    pub id: i64,
    pub world_name: String,
    pub address: String,
    pub action: String,
    pub room: String,
    pub created_at: DateTime<Utc>,
}
