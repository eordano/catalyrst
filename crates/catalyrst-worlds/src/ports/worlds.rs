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
            r#"SELECT name, owner, access, blocked_since, spawn_coordinates
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
}
