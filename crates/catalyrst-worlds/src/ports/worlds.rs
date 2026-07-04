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

    pub async fn list_index_scenes(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(String, WorldScene)>, ApiError> {
        let rows = sqlx::query(
            r#"WITH paged_worlds AS (
                 SELECT DISTINCT ws.world_name
                 FROM world_scenes ws
                 JOIN worlds w ON lower(w.name) = lower(ws.world_name)
                 WHERE w.blocked_since IS NULL
                 ORDER BY ws.world_name
                 LIMIT $1 OFFSET $2
               )
               SELECT ws.world_name, ws.entity_id, ws.entity, ws.parcels
               FROM world_scenes ws
               JOIN paged_worlds pw ON pw.world_name = ws.world_name
               ORDER BY ws.world_name, ws.created_at DESC"#,
        )
        .bind(limit)
        .bind(offset)
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

    #[allow(clippy::too_many_arguments)]
    pub async fn deploy_scene(
        &self,
        world_name: &str,
        owner: &str,
        entity_id: &str,
        deployer: &str,
        deployment_auth_chain: &Value,
        entity: &Value,
        parcels: &[String],
        size: i64,
    ) -> Result<(), ApiError> {
        let mut tx = self.pool.begin().await?;

        let s = scene_settings_from_entity(entity);
        sqlx::query(
            r#"INSERT INTO worlds (
                   name, owner, blocked_since, spawn_coordinates,
                   title, description, content_rating, skybox_time, categories,
                   single_player, show_in_places, thumbnail_hash, updated_at
               )
               VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8::text[], $9, $10, $11, now())
               ON CONFLICT (name) DO UPDATE SET
                 owner = EXCLUDED.owner,
                 blocked_since = NULL,
                 spawn_coordinates = COALESCE(worlds.spawn_coordinates, EXCLUDED.spawn_coordinates),
                 updated_at = now()"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(&s.spawn_coordinates)
        .bind(&s.title)
        .bind(&s.description)
        .bind(&s.content_rating)
        .bind(s.skybox_time)
        .bind(&s.categories)
        .bind(s.single_player)
        .bind(s.show_in_places)
        .bind(&s.thumbnail_hash)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"DELETE FROM world_scenes
               WHERE lower(world_name) = lower($1) AND parcels && $2"#,
        )
        .bind(world_name)
        .bind(parcels)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO world_scenes
                 (world_name, entity_id, deployment_auth_chain, entity, deployer, parcels, size)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (world_name, entity_id) DO UPDATE
                 SET deployment_auth_chain = EXCLUDED.deployment_auth_chain,
                     entity = EXCLUDED.entity,
                     deployer = EXCLUDED.deployer,
                     parcels = EXCLUDED.parcels,
                     size = EXCLUDED.size,
                     created_at = now()"#,
        )
        .bind(world_name)
        .bind(entity_id)
        .bind(deployment_auth_chain)
        .bind(entity)
        .bind(deployer)
        .bind(parcels)
        .bind(size)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn undeploy_scene(&self, world_name: &str, parcel: &str) -> Result<u64, ApiError> {
        let res = sqlx::query(
            r#"DELETE FROM world_scenes
               WHERE lower(world_name) = lower($1) AND $2 = ANY(parcels)"#,
        )
        .bind(world_name)
        .bind(parcel)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn list_scenes(
        &self,
        world_name: &str,
    ) -> Result<Vec<(String, Vec<String>)>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT entity_id, parcels FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY entity_id"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("entity_id"),
                    r.get::<Vec<String>, _>("parcels"),
                )
            })
            .collect())
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

    pub async fn admin_unblock_wallet(&self, wallet: &str) -> Result<bool, ApiError> {
        let res = sqlx::query(r#"DELETE FROM blocked WHERE lower(wallet) = lower($1)"#)
            .bind(wallet)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

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

    pub async fn create_basic_world_if_not_exists(
        &self,
        world_name: &str,
        owner: &str,
    ) -> Result<(), ApiError> {
        let default_access = serde_json::json!({ "type": "unrestricted" });
        sqlx::query(
            r#"INSERT INTO worlds (name, owner, access, created_at, updated_at)
               VALUES (lower($1), lower($2), $3::jsonb, now(), now())
               ON CONFLICT (name) DO NOTHING"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(&default_access)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_worlds_public(
        &self,
        filters: &WorldsListFilters,
        options: &WorldsListOptions,
    ) -> Result<(Vec<WorldInfoRow>, i64), ApiError> {
        let base_from = r#"
            FROM worlds w
            LEFT JOIN (
                SELECT ws.world_name,
                       count(DISTINCT ws.entity_id) AS deployed_scenes,
                       max(ws.created_at) AS last_deployed_at,
                       min(split_part(p, ',', 1)::int) AS min_x,
                       max(split_part(p, ',', 1)::int) AS max_x,
                       min(split_part(p, ',', 2)::int) AS min_y,
                       max(split_part(p, ',', 2)::int) AS max_y
                FROM world_scenes ws, unnest(ws.parcels) AS p
                GROUP BY ws.world_name
            ) ss ON lower(ss.world_name) = lower(w.name)
            LEFT JOIN blocked b ON w.owner = b.wallet
            WHERE ($1::text IS NULL
                    OR w.owner = lower($1)
                    OR EXISTS (SELECT 1 FROM world_permissions wp
                                WHERE lower(wp.world_name) = lower(w.name)
                                  AND wp.address = lower($1)
                                  AND wp.permission_type = 'deployment'))
              AND ($2::bool IS NULL OR (COALESCE(ss.deployed_scenes, 0) > 0) = $2)
              AND ($3::text IS NULL
                    OR w.name ILIKE '%' || $3 || '%'
                    OR w.title ILIKE '%' || $3 || '%'
                    OR w.description ILIKE '%' || $3 || '%')
        "#;

        let dir = match options.order_direction {
            OrderDirection::Desc => "DESC",
            OrderDirection::Asc => "ASC",
        };
        let order_clause = match options.order_by {
            WorldsOrderBy::LastDeployedAt => format!(
                "ORDER BY ss.last_deployed_at IS NULL ASC, ss.last_deployed_at {dir}, w.name ASC"
            ),
            WorldsOrderBy::Name => format!("ORDER BY w.name {dir}"),
        };

        let count_sql = format!("SELECT count(*) AS total {base_from}");
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
            .bind(&filters.authorized_deployer)
            .bind(filters.has_deployed_scenes)
            .bind(&filters.search)
            .fetch_one(&self.pool)
            .await?;

        let main_sql = format!(
            r#"SELECT w.name, w.owner, w.title, w.description, w.content_rating,
                      w.spawn_coordinates, w.skybox_time, w.categories, w.single_player,
                      w.show_in_places, w.thumbnail_hash,
                      ss.last_deployed_at,
                      ss.min_x, ss.max_x, ss.min_y, ss.max_y,
                      b.created_at AS blocked_since,
                      COALESCE(ss.deployed_scenes, 0) AS deployed_scenes
               {base_from}
               {order_clause}
               LIMIT $4 OFFSET $5"#
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(main_sql))
            .bind(&filters.authorized_deployer)
            .bind(filters.has_deployed_scenes)
            .bind(&filters.search)
            .bind(options.limit)
            .bind(options.offset)
            .fetch_all(&self.pool)
            .await?;

        let worlds = rows
            .into_iter()
            .map(|r| WorldInfoRow {
                name: r.get("name"),
                owner: r.get("owner"),
                title: r.get("title"),
                description: r.get("description"),
                content_rating: r.get("content_rating"),
                spawn_coordinates: r.get("spawn_coordinates"),
                skybox_time: r.get("skybox_time"),
                categories: r.get("categories"),
                single_player: r.get("single_player"),
                show_in_places: r.get("show_in_places"),
                thumbnail_hash: r.get("thumbnail_hash"),
                last_deployed_at: r.get("last_deployed_at"),
                min_x: r.get("min_x"),
                max_x: r.get("max_x"),
                min_y: r.get("min_y"),
                max_y: r.get("max_y"),
                blocked_since: r.get("blocked_since"),
                deployed_scenes: r.get("deployed_scenes"),
            })
            .collect();

        Ok((worlds, total))
    }

    pub async fn get_world_settings(
        &self,
        world_name: &str,
    ) -> Result<Option<WorldSettingsRow>, ApiError> {
        let row = sqlx::query(
            r#"SELECT title, description, content_rating, spawn_coordinates, skybox_time,
                      categories, single_player, show_in_places, thumbnail_hash
               FROM worlds WHERE lower(name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| WorldSettingsRow {
            title: r.get("title"),
            description: r.get("description"),
            content_rating: r.get("content_rating"),
            spawn_coordinates: r.get("spawn_coordinates"),
            skybox_time: r.get("skybox_time"),
            categories: r.get("categories"),
            single_player: r.get("single_player"),
            show_in_places: r.get("show_in_places"),
            thumbnail_hash: r.get("thumbnail_hash"),
        }))
    }

    pub async fn get_world_bounding_rectangle(
        &self,
        world_name: &str,
    ) -> Result<Option<(i32, i32, i32, i32)>, ApiError> {
        let row = sqlx::query(
            r#"SELECT min(split_part(p, ',', 1)::int) AS min_x,
                      max(split_part(p, ',', 1)::int) AS max_x,
                      min(split_part(p, ',', 2)::int) AS min_y,
                      max(split_part(p, ',', 2)::int) AS max_y
               FROM world_scenes ws, unnest(ws.parcels) AS p
               WHERE lower(ws.world_name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| {
            let min_x: Option<i32> = r.get("min_x");
            let max_x: Option<i32> = r.get("max_x");
            let min_y: Option<i32> = r.get("min_y");
            let max_y: Option<i32> = r.get("max_y");
            match (min_x, max_x, min_y, max_y) {
                (Some(a), Some(b), Some(c), Some(d)) => Some((a, b, c, d)),
                _ => None,
            }
        }))
    }

    pub async fn update_world_settings(
        &self,
        world_name: &str,
        owner: &str,
        input: &WorldSettingsUpdate,
    ) -> Result<(WorldSettingsRow, Option<String>), ApiError> {
        let mut tx = self.pool.begin().await?;

        let old_spawn: Option<String> = sqlx::query_scalar(
            r#"SELECT spawn_coordinates FROM worlds WHERE lower(name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();

        let skybox_provided = input.skybox_time_provided;
        let categories: Option<Vec<String>> = if input.categories_provided {
            Some(input.categories.clone().unwrap_or_default())
        } else {
            None
        };
        let default_access = serde_json::json!({ "type": "unrestricted" });

        let row = sqlx::query(
            r#"INSERT INTO worlds (
                   name, owner, access,
                   title, description, content_rating, spawn_coordinates,
                   skybox_time, categories, single_player, show_in_places, thumbnail_hash,
                   created_at, updated_at
               )
               VALUES (lower($1), lower($2), $3::jsonb,
                       $4, $5, $6, $7, $8, $9::text[], $10, $11, $12, now(), now())
               ON CONFLICT (name) DO UPDATE SET
                 title = COALESCE(EXCLUDED.title, worlds.title),
                 description = COALESCE(EXCLUDED.description, worlds.description),
                 content_rating = COALESCE(EXCLUDED.content_rating, worlds.content_rating),
                 spawn_coordinates = COALESCE(EXCLUDED.spawn_coordinates, worlds.spawn_coordinates),
                 skybox_time = CASE WHEN $13::boolean THEN EXCLUDED.skybox_time
                                    ELSE COALESCE(EXCLUDED.skybox_time, worlds.skybox_time) END,
                 categories = COALESCE(EXCLUDED.categories, worlds.categories),
                 single_player = COALESCE(EXCLUDED.single_player, worlds.single_player),
                 show_in_places = COALESCE(EXCLUDED.show_in_places, worlds.show_in_places),
                 thumbnail_hash = COALESCE(EXCLUDED.thumbnail_hash, worlds.thumbnail_hash),
                 updated_at = now()
               RETURNING title, description, content_rating, spawn_coordinates, skybox_time,
                         categories, single_player, show_in_places, thumbnail_hash"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(&default_access)
        .bind(&input.title)
        .bind(&input.description)
        .bind(&input.content_rating)
        .bind(&input.spawn_coordinates)
        .bind(input.skybox_time)
        .bind(&categories)
        .bind(input.single_player)
        .bind(input.show_in_places)
        .bind(&input.thumbnail_hash)
        .bind(skybox_provided)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok((
            WorldSettingsRow {
                title: row.get("title"),
                description: row.get("description"),
                content_rating: row.get("content_rating"),
                spawn_coordinates: row.get("spawn_coordinates"),
                skybox_time: row.get("skybox_time"),
                categories: row.get("categories"),
                single_player: row.get("single_player"),
                show_in_places: row.get("show_in_places"),
                thumbnail_hash: row.get("thumbnail_hash"),
            },
            old_spawn,
        ))
    }

    pub async fn get_world_manifest(
        &self,
        world_name: &str,
    ) -> Result<Option<WorldManifest>, ApiError> {
        const PARCELS_LIMIT: i64 = 500;

        let total: i64 = sqlx::query_scalar(
            r#"SELECT count(DISTINCT p)
               FROM world_scenes ws, unnest(ws.parcels) AS p
               WHERE lower(ws.world_name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_one(&self.pool)
        .await?;

        if total == 0 {
            return Ok(None);
        }

        let rows = sqlx::query(
            r#"SELECT parcel
               FROM (
                   SELECT DISTINCT p AS parcel
                   FROM world_scenes ws, unnest(ws.parcels) AS p
                   WHERE lower(ws.world_name) = lower($1)
               ) sub
               ORDER BY split_part(parcel, ',', 1)::int, split_part(parcel, ',', 2)::int
               LIMIT $2"#,
        )
        .bind(world_name)
        .bind(PARCELS_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        let parcels: Vec<String> = rows.into_iter().map(|r| r.get("parcel")).collect();

        let spawn: Option<String> = sqlx::query_scalar(
            r#"SELECT spawn_coordinates FROM worlds WHERE lower(name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        Ok(Some(WorldManifest {
            parcels,
            spawn_coordinates: spawn,
            total,
        }))
    }

    pub async fn get_world_permission_records_full(
        &self,
        world_name: &str,
    ) -> Result<Vec<PermissionRecordFull>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT wp.id,
                      wp.permission_type,
                      wp.address,
                      count(wpp.parcel) = 0 AS is_world_wide,
                      count(wpp.parcel) AS parcel_count
               FROM world_permissions wp
               LEFT JOIN world_permission_parcels wpp ON wp.id = wpp.permission_id
               WHERE lower(wp.world_name) = lower($1)
               GROUP BY wp.id, wp.permission_type, wp.address
               ORDER BY wp.address, wp.permission_type"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PermissionRecordFull {
                id: r.get("id"),
                permission_type: r.get("permission_type"),
                address: r.get("address"),
                is_world_wide: r.get("is_world_wide"),
                parcel_count: r.get("parcel_count"),
            })
            .collect())
    }

    pub async fn grant_addresses_world_wide_permission(
        &self,
        world_name: &str,
        permission: &str,
        addresses: &[String],
    ) -> Result<Vec<String>, ApiError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        let mut tx = self.pool.begin().await?;

        let inserted = sqlx::query(
            r#"INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
               SELECT lower($1), $2, addr, now(), now() FROM unnest($3::text[]) AS addr
               ON CONFLICT (world_name, permission_type, address) DO NOTHING
               RETURNING address"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&lowered)
        .fetch_all(&mut *tx)
        .await?;
        let added: Vec<String> = inserted.into_iter().map(|r| r.get("address")).collect();

        sqlx::query(
            r#"DELETE FROM world_permission_parcels
               WHERE permission_id IN (
                 SELECT id FROM world_permissions
                 WHERE lower(world_name) = lower($1)
                   AND permission_type = $2
                   AND address = ANY($3::text[])
               )"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&lowered)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(added)
    }

    pub async fn remove_addresses_permission(
        &self,
        world_name: &str,
        permission: &str,
        addresses: &[String],
    ) -> Result<Vec<String>, ApiError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        let rows = sqlx::query(
            r#"DELETE FROM world_permissions
               WHERE lower(world_name) = lower($1)
                 AND permission_type = $2
                 AND address = ANY($3::text[])
               RETURNING address"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&lowered)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get("address")).collect())
    }

    pub async fn get_address_permission_id(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
    ) -> Result<Option<i32>, ApiError> {
        Ok(sqlx::query_scalar(
            r#"SELECT id FROM world_permissions
               WHERE lower(world_name) = lower($1)
                 AND permission_type = $2
                 AND address = lower($3)"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn add_parcels_to_permission(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
        parcels: &[String],
    ) -> Result<bool, ApiError> {
        let canon = canonicalize_parcels(parcels);
        let mut tx = self.pool.begin().await?;

        let existing: Option<i32> = sqlx::query_scalar(
            r#"SELECT id FROM world_permissions
               WHERE lower(world_name) = lower($1) AND permission_type = $2 AND address = lower($3)"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .fetch_optional(&mut *tx)
        .await?;

        let (permission_id, created) = match existing {
            Some(id) => {
                sqlx::query(r#"UPDATE world_permissions SET updated_at = now() WHERE id = $1"#)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                (id, false)
            }
            None => {
                let id: i32 = sqlx::query_scalar(
                    r#"INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
                       VALUES (lower($1), $2, lower($3), now(), now())
                       RETURNING id"#,
                )
                .bind(world_name)
                .bind(permission)
                .bind(address)
                .fetch_one(&mut *tx)
                .await?;
                (id, true)
            }
        };

        if !canon.is_empty() {
            sqlx::query(
                r#"INSERT INTO world_permission_parcels (permission_id, parcel)
                   SELECT $1, parcel FROM unnest($2::text[]) AS parcel
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(permission_id)
            .bind(&canon)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(created)
    }

    pub async fn remove_parcels_from_permission(
        &self,
        permission_id: i32,
        parcels: &[String],
    ) -> Result<(), ApiError> {
        if parcels.is_empty() {
            return Ok(());
        }
        let canon = canonicalize_parcels(parcels);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"DELETE FROM world_permission_parcels
               WHERE permission_id = $1 AND parcel = ANY($2::text[])"#,
        )
        .bind(permission_id)
        .bind(&canon)
        .execute(&mut *tx)
        .await?;
        sqlx::query(r#"UPDATE world_permissions SET updated_at = now() WHERE id = $1"#)
            .bind(permission_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_parcels_for_permission(
        &self,
        permission_id: i32,
        limit: i64,
        offset: i64,
        bbox: Option<(i32, i32, i32, i32)>,
    ) -> Result<(i64, Vec<String>), ApiError> {
        let (has_bbox, min_x, max_x, min_y, max_y) = match bbox {
            Some((x1, y1, x2, y2)) => (true, x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2)),
            None => (false, 0, 0, 0, 0),
        };

        let total: i64 = sqlx::query_scalar(
            r#"SELECT count(*) FROM world_permission_parcels
               WHERE permission_id = $1
                 AND ($2::bool = false OR (
                    split_part(parcel, ',', 1)::int BETWEEN $3 AND $4
                    AND split_part(parcel, ',', 2)::int BETWEEN $5 AND $6))"#,
        )
        .bind(permission_id)
        .bind(has_bbox)
        .bind(min_x)
        .bind(max_x)
        .bind(min_y)
        .bind(max_y)
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query(
            r#"SELECT parcel FROM world_permission_parcels
               WHERE permission_id = $1
                 AND ($2::bool = false OR (
                    split_part(parcel, ',', 1)::int BETWEEN $3 AND $4
                    AND split_part(parcel, ',', 2)::int BETWEEN $5 AND $6))
               ORDER BY parcel
               LIMIT $7 OFFSET $8"#,
        )
        .bind(permission_id)
        .bind(has_bbox)
        .bind(min_x)
        .bind(max_x)
        .bind(min_y)
        .bind(max_y)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok((total, rows.into_iter().map(|r| r.get("parcel")).collect()))
    }

    pub async fn get_addresses_for_parcel_permission(
        &self,
        world_name: &str,
        permission: &str,
        parcels: &[String],
        limit: i64,
        offset: i64,
    ) -> Result<(i64, Vec<String>), ApiError> {
        let canon = canonicalize_parcels(parcels);
        let total: i64 = sqlx::query_scalar(
            r#"SELECT count(*) FROM world_permissions wp
               WHERE lower(wp.world_name) = lower($1) AND wp.permission_type = $2
                 AND (NOT EXISTS (SELECT 1 FROM world_permission_parcels wpp WHERE wpp.permission_id = wp.id)
                      OR EXISTS (SELECT 1 FROM world_permission_parcels wpp
                                  WHERE wpp.permission_id = wp.id AND wpp.parcel = ANY($3::text[])))"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&canon)
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query(
            r#"SELECT wp.address FROM world_permissions wp
               WHERE lower(wp.world_name) = lower($1) AND wp.permission_type = $2
                 AND (NOT EXISTS (SELECT 1 FROM world_permission_parcels wpp WHERE wpp.permission_id = wp.id)
                      OR EXISTS (SELECT 1 FROM world_permission_parcels wpp
                                  WHERE wpp.permission_id = wp.id AND wpp.parcel = ANY($3::text[])))
               ORDER BY wp.address
               LIMIT $4 OFFSET $5"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&canon)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok((total, rows.into_iter().map(|r| r.get("address")).collect()))
    }

    pub async fn has_world_wide_permission(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
    ) -> Result<bool, ApiError> {
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM world_permissions wp
                 WHERE lower(wp.world_name) = lower($1)
                   AND wp.permission_type = $2
                   AND wp.address = lower($3)
                   AND NOT EXISTS (SELECT 1 FROM world_permission_parcels wpp
                                    WHERE wpp.permission_id = wp.id)
               )"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    pub async fn store_access(
        &self,
        world_name: &str,
        access: &AccessSetting,
    ) -> Result<(), ApiError> {
        let json = serde_json::to_value(access)
            .map_err(|e| ApiError::internal(format!("serialize access: {e}")))?;
        sqlx::query(
            r#"INSERT INTO worlds (name, access, created_at, updated_at)
               VALUES (lower($1), $2::jsonb, now(), now())
               ON CONFLICT (name) DO UPDATE SET access = $2::jsonb, updated_at = now()"#,
        )
        .bind(world_name)
        .bind(&json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn modify_access_atomically<F>(
        &self,
        world_name: &str,
        modifier: F,
    ) -> Result<AccessSetting, ApiError>
    where
        F: FnOnce(AccessSetting) -> Result<AccessSetting, ApiError>,
    {
        let mut tx = self.pool.begin().await?;
        let row =
            sqlx::query(r#"SELECT access FROM worlds WHERE lower(name) = lower($1) FOR UPDATE"#)
                .bind(world_name)
                .fetch_optional(&mut *tx)
                .await?;
        let current = row
            .and_then(|r| r.get::<Option<Value>, _>("access"))
            .and_then(|v| serde_json::from_value::<AccessSetting>(v).ok())
            .unwrap_or_default();

        let updated = modifier(current)?;
        let json = serde_json::to_value(&updated)
            .map_err(|e| ApiError::internal(format!("serialize access: {e}")))?;
        sqlx::query(
            r#"INSERT INTO worlds (name, access, created_at, updated_at)
               VALUES (lower($1), $2::jsonb, now(), now())
               ON CONFLICT (name) DO UPDATE SET access = $2::jsonb, updated_at = now()"#,
        )
        .bind(world_name)
        .bind(&json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(updated)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldsOrderBy {
    Name,
    LastDeployedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Default)]
pub struct WorldsListFilters {
    pub authorized_deployer: Option<String>,
    pub search: Option<String>,
    pub has_deployed_scenes: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct WorldsListOptions {
    pub limit: i64,
    pub offset: i64,
    pub order_by: WorldsOrderBy,
    pub order_direction: OrderDirection,
}

#[derive(Debug, Clone)]
pub struct WorldInfoRow {
    pub name: String,
    pub owner: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub categories: Option<Vec<String>>,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub min_x: Option<i32>,
    pub max_x: Option<i32>,
    pub min_y: Option<i32>,
    pub max_y: Option<i32>,
    pub blocked_since: Option<DateTime<Utc>>,
    pub deployed_scenes: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WorldSettingsRow {
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub categories: Option<Vec<String>>,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorldSettingsUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub content_rating: Option<String>,
    pub spawn_coordinates: Option<String>,
    pub skybox_time: Option<i32>,
    pub skybox_time_provided: bool,
    pub categories: Option<Vec<String>>,
    pub categories_provided: bool,
    pub single_player: Option<bool>,
    pub show_in_places: Option<bool>,
    pub thumbnail_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorldManifest {
    pub parcels: Vec<String>,
    pub spawn_coordinates: Option<String>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct PermissionRecordFull {
    pub id: i32,
    pub permission_type: String,
    pub address: String,
    pub is_world_wide: bool,
    pub parcel_count: i64,
}

struct DerivedSceneSettings {
    spawn_coordinates: Option<String>,
    title: Option<String>,
    description: Option<String>,
    content_rating: Option<String>,
    skybox_time: Option<i32>,
    categories: Option<Vec<String>>,
    single_player: Option<bool>,
    show_in_places: Option<bool>,
    thumbnail_hash: Option<String>,
}

fn scene_settings_from_entity(entity: &Value) -> DerivedSceneSettings {
    let meta = entity.get("metadata");
    let display = meta.and_then(|m| m.get("display"));
    let wc = meta.and_then(|m| m.get("worldConfiguration"));
    let scene = meta.and_then(|m| m.get("scene"));

    let title = display
        .and_then(|d| d.get("title"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let description = display
        .and_then(|d| d.get("description"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let content_rating = meta
        .and_then(|m| m.get("rating"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let skybox_time = wc
        .and_then(|c| c.get("skyboxConfig"))
        .and_then(|s| s.get("fixedTime"))
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let categories = meta
        .and_then(|m| m.get("tags"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty());
    let single_player = Some(
        wc.and_then(|c| c.get("fixedAdapter"))
            .and_then(|v| v.as_str())
            == Some("offline:offline"),
    );
    let opt_out = wc
        .and_then(|c| c.get("placesConfig"))
        .and_then(|p| p.get("optOut"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let show_in_places = Some(!opt_out);
    let spawn_coordinates = scene
        .and_then(|s| s.get("base"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            scene
                .and_then(|s| s.get("parcels"))
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let thumbnail_hash = display
        .and_then(|d| d.get("navmapThumbnail"))
        .and_then(|v| v.as_str())
        .and_then(|file| {
            entity
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .find(|c| c.get("file").and_then(|f| f.as_str()) == Some(file))
                        .and_then(|c| c.get("hash").and_then(|h| h.as_str()))
                        .map(str::to_string)
                })
        });

    DerivedSceneSettings {
        spawn_coordinates,
        title,
        description,
        content_rating,
        skybox_time,
        categories,
        single_player,
        show_in_places,
        thumbnail_hash,
    }
}

pub fn canonicalize_parcel(s: &str) -> String {
    let parse = |part: &str| -> Option<i64> {
        let t = part.trim();
        let digits = t.strip_prefix('-').unwrap_or(t);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse::<i64>().ok()
    };
    match s.split_once(',') {
        Some((a, b)) => match (parse(a), parse(b)) {
            (Some(x), Some(y)) => format!("{x},{y}"),
            _ => s.to_string(),
        },
        None => s.to_string(),
    }
}

fn canonicalize_parcels(parcels: &[String]) -> Vec<String> {
    parcels.iter().map(|p| canonicalize_parcel(p)).collect()
}
