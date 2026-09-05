use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::access::AccessSetting;
use crate::http::ApiError;

use super::types::{
    canonicalize_parcels, effective_base_parcel, scene_settings_from_entity, AccessLogRow,
    BlockedRow, OrderDirection, PermissionRecordFull, SceneReplacement, WorldAdminRow,
    WorldInfoRow, WorldManifest, WorldRecord, WorldScene, WorldSettingsRow, WorldSettingsUpdate,
    WorldsListFilters, WorldsListOptions, WorldsOrderBy,
};

/// The upsert shared by `store_access` and `modify_access_atomically`: both persist a full
/// replacement of a world's access JSON, differing only in which executor (pool vs. an
/// in-flight transaction) runs it.
async fn upsert_world_access(
    executor: impl sqlx::PgExecutor<'_>,
    world_name: &str,
    access_json: &Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO worlds (name, access, created_at, updated_at)
           VALUES (lower($1), $2::jsonb, now(), now())
           ON CONFLICT (name) DO UPDATE SET access = $2::jsonb,
             settings_version = worlds.settings_version + 1,
             updated_at = now()"#,
    )
    .bind(world_name)
    .bind(access_json)
    .execute(executor)
    .await?;
    Ok(())
}

fn default_access_json() -> Value {
    serde_json::json!({ "type": "unrestricted" })
}

/// The world-shape rectangle spanned by every deployed scene's parcels; runs
/// on the caller's executor so spawn validation can read it under the worlds
/// row lock inside the settings transaction.
async fn bounding_rectangle(
    executor: impl sqlx::PgExecutor<'_>,
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
    .fetch_optional(executor)
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

fn world_scene_from_row(row: &PgRow) -> WorldScene {
    WorldScene {
        entity_id: row.get("entity_id"),
        entity: row.get("entity"),
        parcels: row.get("parcels"),
        deployer: row.get("deployer"),
    }
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
                      skybox_time, single_player, realm_name_override,
                      preview_wearable_urns
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
                realm_name_override: r.get("realm_name_override"),
                preview_wearable_urns: r.get("preview_wearable_urns"),
            }
        }))
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
            r#"SELECT entity_id, entity, parcels, deployer
               FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY created_at DESC"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(world_scene_from_row).collect())
    }

    /// The already-deployed scenes whose parcels overlap `parcels`; a deploy/undeploy must
    /// authorize the full footprint of each before replacing it. `parcels` must already be
    /// canonical, matching how `world_scenes.parcels` are stored.
    pub async fn scenes_overlapping_parcels(
        &self,
        world_name: &str,
        parcels: &[String],
    ) -> Result<Vec<WorldScene>, ApiError> {
        if parcels.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"SELECT entity_id, entity, parcels, deployer
               FROM world_scenes
               WHERE lower(world_name) = lower($1) AND parcels && $2::text[]"#,
        )
        .bind(world_name)
        .bind(parcels)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(world_scene_from_row).collect())
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
               SELECT ws.world_name, ws.entity_id, ws.entity, ws.parcels, ws.deployer
               FROM world_scenes ws
               JOIN paged_worlds pw ON pw.world_name = ws.world_name
               ORDER BY ws.world_name, ws.created_at DESC"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let world_name: String = r.get("world_name");
                (world_name, world_scene_from_row(r))
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
            effective_base_parcel(&entity, &parcels)
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deploy_scene(
        &self,
        world_name: &str,
        name_owner: Option<&str>,
        entity_id: &str,
        deployer: &str,
        deployment_auth_chain: &Value,
        entity: &Value,
        parcels: &[String],
        size: i64,
        contents_dir: &std::path::Path,
        replacement: &SceneReplacement,
    ) -> Result<(), ApiError> {
        let mut s = scene_settings_from_entity(entity);
        // The bytes are checked against the same formats the settings endpoint
        // accepts, since a promoted thumbnail is served verbatim to consumers.
        if let Some(hash) = s.thumbnail_hash.take() {
            s.thumbnail_hash =
                crate::settings_policy::storable_thumbnail_hash(contents_dir, &hash).await;
        }

        let mut tx = self.pool.begin().await?;

        // The upsert takes the worlds row lock; settings columns are written on
        // INSERT (first deploy) but left unchanged on UPDATE -- the refresh
        // decision needs the scene count read AFTER the lock is acquired, so a
        // concurrent deploy cannot race it under READ COMMITTED.
        let is_insert: bool = sqlx::query_scalar(
            r#"INSERT INTO worlds (
                   name, owner, access, blocked_since, spawn_coordinates,
                   title, description, content_rating, skybox_time, categories,
                   single_player, show_in_places, thumbnail_hash, updated_at
               )
               VALUES ($1, COALESCE($2, $12), $13::jsonb, NULL, $3, $4, $5, $6, $7, $8::text[], $9, $10, $11, now())
               ON CONFLICT (name) DO UPDATE SET
                 owner = COALESCE($2, worlds.owner),
                 blocked_since = NULL,
                 spawn_coordinates = COALESCE(worlds.spawn_coordinates, EXCLUDED.spawn_coordinates),
                 updated_at = now()
               RETURNING (xmax = 0)"#,
        )
        .bind(world_name)
        .bind(name_owner)
        .bind(&s.spawn_coordinates)
        .bind(&s.title)
        .bind(&s.description)
        .bind(&s.content_rating)
        .bind(s.skybox_time)
        .bind(&s.categories)
        .bind(s.single_player)
        .bind(s.show_in_places)
        .bind(&s.thumbnail_hash)
        .bind(deployer)
        .bind(default_access_json())
        .fetch_one(&mut *tx)
        .await?;

        if !is_insert {
            // Refresh settings iff no non-overlapping scene survives this deploy:
            // every currently deployed scene is being replaced (or none exist), so
            // the incoming scene ends up alone in the world.
            let sole_occupant: bool = sqlx::query_scalar(
                r#"SELECT COUNT(*) FILTER (WHERE NOT (parcels && $2::text[])) = 0
                   FROM world_scenes WHERE lower(world_name) = lower($1)"#,
            )
            .bind(world_name)
            .bind(parcels)
            .fetch_one(&mut *tx)
            .await?;

            if sole_occupant {
                // A field the scene does not express (NULL) falls back to the
                // stored value; the IS DISTINCT FROM guard keeps a republish of
                // unchanged metadata from bumping the settings version.
                sqlx::query(
                    r#"UPDATE worlds SET
                         title = COALESCE($2, title),
                         description = COALESCE($3, description),
                         content_rating = COALESCE($4, content_rating),
                         skybox_time = COALESCE($5, skybox_time),
                         categories = COALESCE($6::text[], categories),
                         single_player = COALESCE($7, single_player),
                         show_in_places = COALESCE($8, show_in_places),
                         thumbnail_hash = COALESCE($9, thumbnail_hash),
                         settings_version = settings_version + 1,
                         updated_at = now()
                       WHERE lower(name) = lower($1)
                         AND (title, description, content_rating, skybox_time, categories,
                              single_player, show_in_places, thumbnail_hash)
                             IS DISTINCT FROM
                             (COALESCE($2, title), COALESCE($3, description),
                              COALESCE($4, content_rating), COALESCE($5, skybox_time),
                              COALESCE($6::text[], categories), COALESCE($7, single_player),
                              COALESCE($8, show_in_places), COALESCE($9, thumbnail_hash))"#,
                )
                .bind(world_name)
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
            }
        }

        match replacement {
            SceneReplacement::UnrestrictedOwner => {
                sqlx::query(
                    r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND parcels && $2"#,
                )
                .bind(world_name)
                .bind(parcels)
                .execute(&mut *tx)
                .await?;
            }
            SceneReplacement::Scoped(entity_ids) => {
                // Replace only the exact scene identities the caller was authorized for.
                sqlx::query(
                    r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1)
                         AND parcels && $2
                         AND entity_id = ANY($3::text[])"#,
                )
                .bind(world_name)
                .bind(parcels)
                .bind(entity_ids)
                .execute(&mut *tx)
                .await?;

                // The worlds upsert above locks this world's row, serializing deploys; this
                // final overlap probe rejects a scene that appeared after the caller's
                // authorization snapshot was taken, rather than deleting it unauthorized.
                let leftover: Option<String> = sqlx::query_scalar(
                    r#"SELECT entity_id FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND parcels && $2
                       LIMIT 1"#,
                )
                .bind(world_name)
                .bind(parcels)
                .fetch_optional(&mut *tx)
                .await?;
                if leftover.is_some() {
                    return Err(ApiError::conflict(format!(
                        "Scene replacement authorization changed while deploying to world \"{world_name}\". Please retry."
                    )));
                }
            }
        }

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
                     updated_at = now()"#,
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

    /// Undeploy every scene overlapping `parcel`. When `authorized_entity_ids` is set the
    /// delete is scoped to those exact identities, so a parcel-scoped deployer can't remove
    /// a scene reaching into parcels it was never granted; `None` (name owner) is unrestricted.
    pub async fn undeploy_scene(
        &self,
        world_name: &str,
        parcel: &str,
        authorized_entity_ids: Option<&[String]>,
    ) -> Result<u64, ApiError> {
        // Serialize against a concurrent deploy on the same world: take the
        // worlds-row lock before deleting the scene at this parcel.
        let mut tx = self.pool.begin().await?;
        sqlx::query(r#"SELECT name FROM worlds WHERE lower(name) = lower($1) FOR UPDATE"#)
            .bind(world_name)
            .execute(&mut *tx)
            .await?;
        let res = match authorized_entity_ids {
            Some(ids) => {
                sqlx::query(
                    r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND $2 = ANY(parcels)
                         AND entity_id = ANY($3::text[])"#,
                )
                .bind(world_name)
                .bind(parcel)
                .bind(ids)
                .execute(&mut *tx)
                .await?
            }
            None => {
                sqlx::query(
                    r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND $2 = ANY(parcels)"#,
                )
                .bind(world_name)
                .bind(parcel)
                .execute(&mut *tx)
                .await?
            }
        };
        let affected = res.rows_affected();
        tx.commit().await?;
        Ok(affected)
    }

    pub async fn undeploy_world(&self, world_name: &str) -> Result<u64, ApiError> {
        // Serialize against a concurrent deploy on the same world: take the
        // worlds-row lock before deleting its scenes so a deploy cannot slip a
        // scene in between the lock check and the delete.
        let mut tx = self.pool.begin().await?;
        sqlx::query(r#"SELECT name FROM worlds WHERE lower(name) = lower($1) FOR UPDATE"#)
            .bind(world_name)
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query(r#"DELETE FROM world_scenes WHERE lower(world_name) = lower($1)"#)
            .bind(world_name)
            .execute(&mut *tx)
            .await?;
        let affected = res.rows_affected();
        tx.commit().await?;
        Ok(affected)
    }

    pub async fn list_scenes(
        &self,
        world_name: &str,
    ) -> Result<Vec<(String, Vec<String>, Option<String>)>, ApiError> {
        let rows = sqlx::query(
            r#"SELECT entity_id, parcels, entity FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY entity_id"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let entity: Value = r.get("entity");
                let parcels: Vec<String> = r.get("parcels");
                let base = effective_base_parcel(&entity, &parcels);
                (r.get::<String, _>("entity_id"), parcels, base)
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
        sqlx::query(
            r#"INSERT INTO worlds (name, owner, access, created_at, updated_at)
               VALUES (lower($1), lower($2), $3::jsonb, now(), now())
               ON CONFLICT (name) DO NOTHING"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(default_access_json())
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
                       -- A world scene's parcels are not always "x,y": most rows
                       -- carry a bare index ("0", "1"), split_part then yields ''
                       -- and its ::int cast aborts the statement -- which is why
                       -- this endpoint answered 500 to every caller. CASE, not
                       -- FILTER, so the cast is never evaluated for a pointer
                       -- that is not a coordinate pair.
                       min(CASE WHEN p ~ '^-?[0-9]+,-?[0-9]+$'
                                THEN split_part(p, ',', 1)::int END) AS min_x,
                       max(CASE WHEN p ~ '^-?[0-9]+,-?[0-9]+$'
                                THEN split_part(p, ',', 1)::int END) AS max_x,
                       min(CASE WHEN p ~ '^-?[0-9]+,-?[0-9]+$'
                                THEN split_part(p, ',', 2)::int END) AS min_y,
                       max(CASE WHEN p ~ '^-?[0-9]+,-?[0-9]+$'
                                THEN split_part(p, ',', 2)::int END) AS max_y
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
                      w.spawn_coordinates, w.skybox_time, w.categories,
                      COALESCE(w.single_player, false) AS single_player,
                      COALESCE(w.show_in_places, true) AS show_in_places,
                      w.thumbnail_hash,
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
                      categories, single_player, show_in_places, thumbnail_hash,
                      access->>'type' AS access_type, realm_name_override,
                      preview_wearable_urns, settings_version
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
            access_type: r.get("access_type"),
            realm_name_override: r.get("realm_name_override"),
            preview_wearable_urns: r.get("preview_wearable_urns"),
            settings_version: r.get("settings_version"),
        }))
    }

    pub async fn update_world_settings(
        &self,
        world_name: &str,
        owner: &str,
        input: &WorldSettingsUpdate,
    ) -> Result<(WorldSettingsRow, Option<String>), ApiError> {
        let mut tx = self.pool.begin().await?;

        // A spawn coordinate is validated against the world's deployed shape, so
        // the worlds row lock is held across validation and the write -- deploy
        // and undeploy take the same lock before touching world_scenes. FOR
        // UPDATE locks nothing when the row does not exist yet, so materialize
        // it first; a failed validation rolls it back with the transaction.
        if input.spawn_coordinates.is_some() {
            sqlx::query(
                r#"INSERT INTO worlds (name, owner, access, created_at, updated_at)
                   VALUES (lower($1), lower($2), $3::jsonb, now(), now())
                   ON CONFLICT (name) DO NOTHING"#,
            )
            .bind(world_name)
            .bind(owner)
            .bind(default_access_json())
            .execute(&mut *tx)
            .await?;
        }

        let old_spawn: Option<String> = sqlx::query_scalar(
            r#"SELECT spawn_coordinates FROM worlds WHERE lower(name) = lower($1) FOR UPDATE"#,
        )
        .bind(world_name)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();

        if let Some(spawn) = input.spawn_coordinates.as_deref() {
            let (min_x, max_x, min_y, max_y) = bounding_rectangle(&mut *tx, world_name)
                .await?
                .ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "Invalid spawnCoordinates \"{spawn}\". The world has no deployed scenes."
                    ))
                })?;
            let within = catalyrst_types::pointer::parse_pointer(spawn)
                .and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)))
                .map(|(x, y)| (min_x..=max_x).contains(&x) && (min_y..=max_y).contains(&y))
                .unwrap_or(false);
            if !within {
                return Err(ApiError::bad_request(format!(
                    "Invalid spawnCoordinates \"{spawn}\". It must be within the world shape rectangle: ({min_x},{min_y}) to ({max_x},{max_y})."
                )));
            }
        }

        let skybox_provided = input.skybox_time_provided;
        let categories: Option<Vec<String>> = if input.categories_provided {
            Some(input.categories.clone().unwrap_or_default())
        } else {
            None
        };
        // spawn_coordinates deliberately excluded: only settings columns move the
        // version, matching the upstream settings-write policy.
        let has_settings_patch = input.title.is_some()
            || input.description.is_some()
            || input.content_rating.is_some()
            || input.skybox_time_provided
            || input.categories_provided
            || input.single_player.is_some()
            || input.show_in_places.is_some()
            || input.thumbnail_hash.is_some()
            || input.realm_name_override_provided
            || input.preview_wearable_urns_provided;

        let row = sqlx::query(
            r#"INSERT INTO worlds (
                   name, owner, access,
                   title, description, content_rating, spawn_coordinates,
                   skybox_time, categories, single_player, show_in_places, thumbnail_hash,
                   realm_name_override, preview_wearable_urns, created_at, updated_at
               )
               VALUES (lower($1), lower($2), $3::jsonb,
                       $4, $5, $6, $7, $8, $9::text[], $10, $11, $12, $15, $17::text[], now(), now())
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
                 realm_name_override = CASE WHEN $16::boolean THEN EXCLUDED.realm_name_override
                                            ELSE worlds.realm_name_override END,
                 preview_wearable_urns = CASE WHEN $18::boolean THEN EXCLUDED.preview_wearable_urns
                                              ELSE worlds.preview_wearable_urns END,
                 settings_version = CASE WHEN $14::boolean THEN worlds.settings_version + 1
                                         ELSE worlds.settings_version END,
                 updated_at = now()
               RETURNING title, description, content_rating, spawn_coordinates, skybox_time,
                         categories, single_player, show_in_places, thumbnail_hash,
                         access->>'type' AS access_type, realm_name_override,
                         preview_wearable_urns, settings_version"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(default_access_json())
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
        .bind(has_settings_patch)
        .bind(&input.realm_name_override)
        .bind(input.realm_name_override_provided)
        .bind(&input.preview_wearable_urns)
        .bind(input.preview_wearable_urns_provided)
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
                access_type: row.get("access_type"),
                realm_name_override: row.get("realm_name_override"),
                preview_wearable_urns: row.get("preview_wearable_urns"),
                settings_version: row.get("settings_version"),
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
        upsert_world_access(&self.pool, world_name, &json).await
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
        upsert_world_access(&mut *tx, world_name, &json).await?;
        tx.commit().await?;
        Ok(updated)
    }
}
