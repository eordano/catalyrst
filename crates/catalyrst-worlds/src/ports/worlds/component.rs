use serde_json::Value;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::{PgPool, Row};

use crate::access::AccessSetting;
use crate::http::ApiError;

use super::types::{
    canonicalize_parcels, effective_base_parcel, scene_settings_from_entity, AccessLogRow,
    AllowListEdit, AllowListEditOutcome, BlockedRow, OrderDirection, PermissionRecordFull,
    SceneReplacement, WorldAbout, WorldAdminRow, WorldInfoRow, WorldLookup, WorldManifest,
    WorldProbe, WorldRecord, WorldScene, WorldSettingsRow, WorldSettingsUpdate, WorldsListFilters,
    WorldsListOptions, WorldsOrderBy, MAX_ACCESS_COMMUNITIES, MAX_ACCESS_WALLETS,
};

type PgQuery<'q> = sqlx::query::Query<'q, sqlx::Postgres, PgArguments>;

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

async fn ensure_world(
    executor: impl sqlx::PgExecutor<'_>,
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
    .execute(executor)
    .await?;
    Ok(())
}

async fn lock_world(executor: impl sqlx::PgExecutor<'_>, world_name: &str) -> Result<(), ApiError> {
    sqlx::query(r#"SELECT name FROM worlds WHERE lower(name) = lower($1) FOR UPDATE"#)
        .bind(world_name)
        .execute(executor)
        .await?;
    Ok(())
}

fn default_access_json() -> Value {
    serde_json::json!({ "type": "unrestricted" })
}

fn access_json(access: &AccessSetting) -> Result<Value, ApiError> {
    serde_json::to_value(access).map_err(|e| ApiError::internal(format!("serialize access: {e}")))
}

fn access_setting(r: &PgRow) -> AccessSetting {
    r.get::<Option<Value>, _>("access")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// The world-shape rectangle spanned by every deployed scene's parcels; runs
/// on the caller's executor so spawn validation can read it under the worlds
/// row lock inside the settings transaction.
fn world_scene_from_row(row: &PgRow) -> WorldScene {
    WorldScene {
        entity_id: row.get("entity_id"),
        entity: row.get("entity"),
        parcels: row.get("parcels"),
        deployer: row.get("deployer"),
    }
}

fn parcels_and_base(r: &PgRow) -> (Vec<String>, Option<String>) {
    let entity: Value = r.get("entity");
    let parcels: Vec<String> = r.get("parcels");
    let base = effective_base_parcel(&entity, &parcels);
    (parcels, base)
}

fn world_settings_from_row(r: &PgRow) -> WorldSettingsRow {
    WorldSettingsRow {
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
    }
}

const ABOUT_MEMO_TTL: std::time::Duration = std::time::Duration::from_secs(10);
const ABOUT_MEMO_CAPACITY: u64 = 10_000;

/// Rows per multi-row access-log INSERT; the drain task takes whatever is queued up to this.
const ACCESS_LOG_BATCH: usize = 256;

struct AccessLogEntry {
    world_name: String,
    address: String,
    action: String,
    room: String,
}

#[derive(Clone)]
pub struct WorldsComponent {
    pool: PgPool,
    /// `/world/{name}/about` snapshot keyed by lower(name); every local write to the
    /// world drops it, the TTL covers writers in other processes (mirror, storage).
    about_memo: moka::future::Cache<String, std::sync::Arc<WorldAbout>>,
    /// Lazily started drain task for `record_access_queued`, shared by every clone.
    access_log:
        std::sync::Arc<std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<AccessLogEntry>>>,
}

async fn drain_access_log(
    pool: PgPool,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AccessLogEntry>,
) {
    let mut batch = Vec::with_capacity(ACCESS_LOG_BATCH);
    while rx.recv_many(&mut batch, ACCESS_LOG_BATCH).await > 0 {
        let n = batch.len();
        let (mut worlds, mut addresses, mut actions, mut rooms) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for e in batch.drain(..) {
            worlds.push(e.world_name);
            addresses.push(e.address.to_lowercase());
            actions.push(e.action);
            rooms.push(e.room);
        }
        let res = sqlx::query(
            r#"INSERT INTO world_access_log (world_name, address, action, room)
               SELECT * FROM unnest($1::text[], $2::text[], $3::text[], $4::text[])"#,
        )
        .bind(&worlds)
        .bind(&addresses)
        .bind(&actions)
        .bind(&rooms)
        .execute(&pool)
        .await;
        if let Err(e) = res {
            tracing::warn!(error = %e, rows = n, "failed to persist world access log rows");
        }
    }
}

impl WorldsComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            about_memo: moka::future::Cache::builder()
                .max_capacity(ABOUT_MEMO_CAPACITY)
                .time_to_live(ABOUT_MEMO_TTL)
                .build(),
            access_log: std::sync::Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Queues an access-log row for the batched drain task (started on first use, so
    /// this must run inside the tokio runtime). Failures were only ever logged.
    pub fn record_access_queued(&self, world_name: &str, address: &str, action: &str, room: &str) {
        let tx = self.access_log.get_or_init(|| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(drain_access_log(self.pool.clone(), rx));
            tx
        });
        let entry = AccessLogEntry {
            world_name: world_name.to_string(),
            address: address.to_string(),
            action: action.to_string(),
            room: room.to_string(),
        };
        if tx.send(entry).is_err() {
            tracing::warn!("world access log drain task is gone; row dropped");
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn forget_about(&self, world_name: &str) {
        self.about_memo.invalidate(&world_name.to_lowercase()).await;
    }

    /// The world row (if any) and its scenes newest-first in one statement; either side
    /// may be absent independently.
    pub async fn get_world_with_scenes(&self, world_name: &str) -> Result<WorldAbout, ApiError> {
        let rows = sqlx::query(
            r#"SELECT w.name, w.owner, w.access, w.blocked_since, w.spawn_coordinates,
                      w.skybox_time, w.single_player, w.realm_name_override,
                      w.preview_wearable_urns,
                      s.entity_id, s.entity, s.parcels, s.deployer
               FROM (SELECT * FROM worlds WHERE lower(name) = lower($1)) w
               FULL OUTER JOIN (
                 SELECT entity_id, entity, parcels, deployer, created_at
                 FROM world_scenes WHERE lower(world_name) = lower($1)
               ) s ON true
               ORDER BY s.created_at DESC NULLS LAST"#,
        )
        .bind(world_name)
        .fetch_all(&self.pool)
        .await?;

        let world = rows.first().and_then(|r| {
            r.get::<Option<String>, _>("name").map(|name| WorldRecord {
                name,
                owner: r.get("owner"),
                access: access_setting(r),
                blocked_since: r.get("blocked_since"),
                spawn_coordinates: r.get("spawn_coordinates"),
                skybox_time: r.get("skybox_time"),
                single_player: r.get::<Option<bool>, _>("single_player").unwrap_or(false),
                realm_name_override: r.get("realm_name_override"),
                preview_wearable_urns: r.get("preview_wearable_urns"),
            })
        });
        let scenes = rows
            .iter()
            .filter(|r| r.get::<Option<String>, _>("entity_id").is_some())
            .map(world_scene_from_row)
            .collect();
        Ok(WorldAbout { world, scenes })
    }

    pub async fn world_about(
        &self,
        world_name: &str,
    ) -> Result<std::sync::Arc<WorldAbout>, ApiError> {
        let key = world_name.to_lowercase();
        self.about_memo
            .try_get_with(key, async {
                self.get_world_with_scenes(world_name)
                    .await
                    .map(std::sync::Arc::new)
            })
            .await
            .map_err(|e| ApiError::internal(e.to_string()))
    }

    async fn scenes(&self, query: PgQuery<'_>) -> Result<Vec<WorldScene>, ApiError> {
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.iter().map(world_scene_from_row).collect())
    }

    /// Runs `delete` against `world_scenes` under the world's row lock and reports the
    /// number of rows it removed.
    async fn delete_scenes_locked(
        &self,
        world_name: &str,
        delete: PgQuery<'_>,
    ) -> Result<u64, ApiError> {
        let mut tx = self.pool.begin().await?;
        lock_world(&mut *tx, world_name).await?;
        let affected = delete.execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        self.forget_about(world_name).await;
        Ok(affected)
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

        Ok(row.map(|r| WorldRecord {
            name: r.get("name"),
            owner: r.get("owner"),
            access: access_setting(&r),
            blocked_since: r.get("blocked_since"),
            spawn_coordinates: r.get("spawn_coordinates"),
            skybox_time: r.get("skybox_time"),
            single_player: r.get::<Option<bool>, _>("single_player").unwrap_or(false),
            realm_name_override: r.get("realm_name_override"),
            preview_wearable_urns: r.get("preview_wearable_urns"),
        }))
    }

    /// `get_world` plus the per-request companions that used to be separate statements:
    /// the wallet's `blocked` row, a world-wide deployment grant, a scenes-exist flag and
    /// one scene's base parcel. Each probe is evaluated only when requested.
    pub async fn lookup_world(
        &self,
        world_name: &str,
        probe: WorldProbe<'_>,
    ) -> Result<WorldLookup, ApiError> {
        let r = sqlx::query(
            r#"SELECT w.name, w.owner, w.access, w.blocked_since, w.spawn_coordinates,
                      w.skybox_time, w.single_player, w.realm_name_override,
                      w.preview_wearable_urns,
                      ($2::text IS NOT NULL AND EXISTS(
                        SELECT 1 FROM blocked WHERE lower(wallet) = lower($2))) AS wallet_blocked,
                      ($3::text IS NOT NULL AND EXISTS(
                        SELECT 1 FROM world_permissions wp
                        WHERE lower(wp.world_name) = lower($1)
                          AND wp.permission_type = 'deployment'
                          AND wp.address = lower($3)
                          AND NOT EXISTS (SELECT 1 FROM world_permission_parcels wpp
                                           WHERE wpp.permission_id = wp.id))) AS world_wide_deployer,
                      ($5::boolean AND EXISTS(
                        SELECT 1 FROM world_scenes WHERE lower(world_name) = lower($1))) AS has_scenes,
                      s.entity AS scene_entity, s.parcels AS scene_parcels
               FROM (SELECT 1) AS one
               LEFT JOIN worlds w ON lower(w.name) = lower($1)
               LEFT JOIN world_scenes s ON $4::text IS NOT NULL
                    AND lower(s.world_name) = lower($1) AND s.entity_id = $4"#,
        )
        .bind(world_name)
        .bind(probe.wallet_blocked)
        .bind(probe.world_wide_deployer)
        .bind(probe.scene_id)
        .bind(probe.has_scenes)
        .fetch_one(&self.pool)
        .await?;

        let world = r.get::<Option<String>, _>("name").map(|name| WorldRecord {
            name,
            owner: r.get("owner"),
            access: access_setting(&r),
            blocked_since: r.get("blocked_since"),
            spawn_coordinates: r.get("spawn_coordinates"),
            skybox_time: r.get("skybox_time"),
            single_player: r.get::<Option<bool>, _>("single_player").unwrap_or(false),
            realm_name_override: r.get("realm_name_override"),
            preview_wearable_urns: r.get("preview_wearable_urns"),
        });
        let scene_base = match (
            r.get::<Option<Value>, _>("scene_entity"),
            r.get::<Option<Vec<String>>, _>("scene_parcels"),
        ) {
            (Some(entity), Some(parcels)) => effective_base_parcel(&entity, &parcels),
            _ => None,
        };
        Ok(WorldLookup {
            world,
            wallet_blocked: r.get("wallet_blocked"),
            world_wide_deployer: r.get("world_wide_deployer"),
            has_scenes: r.get("has_scenes"),
            scene_base,
        })
    }

    pub async fn is_world_valid(&self, world_name: &str) -> Result<bool, ApiError> {
        Ok(sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM world_scenes WHERE lower(world_name) = lower($1)
               )"#,
        )
        .bind(world_name)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_scenes(&self, world_name: &str) -> Result<Vec<WorldScene>, ApiError> {
        self.scenes(
            sqlx::query(
                r#"SELECT entity_id, entity, parcels, deployer
               FROM world_scenes
               WHERE lower(world_name) = lower($1)
               ORDER BY created_at DESC"#,
            )
            .bind(world_name),
        )
        .await
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
        self.scenes(
            sqlx::query(
                r#"SELECT entity_id, entity, parcels, deployer
               FROM world_scenes
               WHERE lower(world_name) = lower($1) AND parcels && $2::text[]"#,
            )
            .bind(world_name)
            .bind(parcels),
        )
        .await
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
               SELECT ws.world_name, ws.entity_id, ws.parcels, ws.deployer,
                      jsonb_build_object(
                        'timestamp', ws.entity->'timestamp',
                        'metadata', jsonb_build_object(
                          'display', ws.entity->'metadata'->'display',
                          'runtimeVersion', ws.entity->'metadata'->'runtimeVersion'),
                        'content', (SELECT jsonb_agg(c) FROM jsonb_array_elements(
                                      CASE WHEN jsonb_typeof(ws.entity->'content') = 'array'
                                           THEN ws.entity->'content' ELSE '[]'::jsonb END) c
                                    WHERE c->>'file' = ws.entity->'metadata'->'display'->>'navmapThumbnail')
                      ) AS entity
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
            .map(|r| (r.get("world_name"), world_scene_from_row(r)))
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

        Ok(row.and_then(|r| parcels_and_base(&r).1))
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
        if let Some(hash) = s.thumbnail_hash.take() {
            s.thumbnail_hash =
                crate::settings_policy::storable_thumbnail_hash(contents_dir, &hash).await;
        }

        let mut tx = self.pool.begin().await?;

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
            let sole_occupant: bool = sqlx::query_scalar(
                r#"SELECT COUNT(*) FILTER (WHERE NOT (parcels && $2::text[])) = 0
                   FROM world_scenes WHERE lower(world_name) = lower($1)"#,
            )
            .bind(world_name)
            .bind(parcels)
            .fetch_one(&mut *tx)
            .await?;

            if sole_occupant {
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
        self.forget_about(world_name).await;
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
        let delete = match authorized_entity_ids {
            Some(ids) => sqlx::query(
                r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND $2 = ANY(parcels)
                         AND entity_id = ANY($3::text[])"#,
            )
            .bind(world_name)
            .bind(parcel)
            .bind(ids),
            None => sqlx::query(
                r#"DELETE FROM world_scenes
                       WHERE lower(world_name) = lower($1) AND $2 = ANY(parcels)"#,
            )
            .bind(world_name)
            .bind(parcel),
        };
        self.delete_scenes_locked(world_name, delete).await
    }

    pub async fn undeploy_world(&self, world_name: &str) -> Result<u64, ApiError> {
        self.delete_scenes_locked(
            world_name,
            sqlx::query(r#"DELETE FROM world_scenes WHERE lower(world_name) = lower($1)"#)
                .bind(world_name),
        )
        .await
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
                let (parcels, base) = parcels_and_base(&r);
                (r.get("entity_id"), parcels, base)
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
        Ok(sqlx::query_scalar(
            r#"SELECT EXISTS(
                 SELECT 1 FROM blocked WHERE lower(wallet) = lower($1)
               )"#,
        )
        .bind(wallet)
        .fetch_one(&self.pool)
        .await?)
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
            .map(|r| WorldAdminRow {
                name: r.get("name"),
                owner: r.get("owner"),
                access_type: r
                    .get::<Option<Value>, _>("access")
                    .and_then(|v| v.get("type")?.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unrestricted".to_string()),
                blocked_since: r.get("blocked_since"),
                spawn_coordinates: r.get("spawn_coordinates"),
                scene_count: r.get("scene_count"),
            })
            .collect())
    }

    /// `GET /admin/worlds` page and total in one statement.
    pub async fn admin_list_worlds_page(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<WorldAdminRow>, i64), ApiError> {
        let rows = sqlx::query(
            r#"SELECT w.name,
                      w.owner,
                      w.access,
                      w.blocked_since,
                      w.spawn_coordinates,
                      COALESCE(sc.scene_count, 0)::bigint AS scene_count,
                      count(*) OVER () AS total
               FROM worlds w
               LEFT JOIN (
                 SELECT lower(world_name) AS lname, count(*) AS scene_count
                 FROM world_scenes GROUP BY lower(world_name)
               ) sc ON sc.lname = lower(w.name)
               ORDER BY w.name
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = match rows.first() {
            Some(r) => r.get("total"),
            None if offset > 0 => self.admin_count_worlds().await?,
            None => 0,
        };
        let worlds = rows
            .into_iter()
            .map(|r| WorldAdminRow {
                name: r.get("name"),
                owner: r.get("owner"),
                access_type: r
                    .get::<Option<Value>, _>("access")
                    .and_then(|v| v.get("type")?.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unrestricted".to_string()),
                blocked_since: r.get("blocked_since"),
                spawn_coordinates: r.get("spawn_coordinates"),
                scene_count: r.get("scene_count"),
            })
            .collect();
        Ok((worlds, total))
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
        self.forget_about(world_name).await;
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
        ensure_world(&self.pool, world_name, owner).await
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

        let main_sql = format!(
            r#"SELECT w.name, w.owner, w.title, w.description, w.content_rating,
                      w.spawn_coordinates, w.skybox_time, w.categories,
                      COALESCE(w.single_player, false) AS single_player,
                      COALESCE(w.show_in_places, true) AS show_in_places,
                      w.thumbnail_hash,
                      ss.last_deployed_at,
                      ss.min_x, ss.max_x, ss.min_y, ss.max_y,
                      b.created_at AS blocked_since,
                      COALESCE(ss.deployed_scenes, 0) AS deployed_scenes,
                      count(*) OVER () AS total
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
        let total: i64 = match rows.first() {
            Some(r) => r.get("total"),
            None if options.offset > 0 => {
                let count_sql = format!("SELECT count(*) AS total {base_from}");
                sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
                    .bind(&filters.authorized_deployer)
                    .bind(filters.has_deployed_scenes)
                    .bind(&filters.search)
                    .fetch_one(&self.pool)
                    .await?
            }
            None => 0,
        };

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

        Ok(row.as_ref().map(world_settings_from_row))
    }

    pub async fn update_world_settings(
        &self,
        world_name: &str,
        owner: &str,
        input: &WorldSettingsUpdate,
    ) -> Result<(WorldSettingsRow, Option<String>), ApiError> {
        let mut tx = self.pool.begin().await?;

        // One locked read: the row (FOR UPDATE) plus, when a spawn is being set, the
        // world's bounding rectangle. Scenes require a worlds row, so a missing row can
        // only ever fail the spawn check; the old pre-insert was rolled back with it.
        let locked = sqlx::query(
            r#"SELECT w.spawn_coordinates, br.min_x, br.max_x, br.min_y, br.max_y
               FROM worlds w
               LEFT JOIN LATERAL (
                 SELECT min(split_part(p, ',', 1)::int) AS min_x,
                        max(split_part(p, ',', 1)::int) AS max_x,
                        min(split_part(p, ',', 2)::int) AS min_y,
                        max(split_part(p, ',', 2)::int) AS max_y
                 FROM world_scenes ws, unnest(ws.parcels) AS p
                 WHERE $2::bool AND lower(ws.world_name) = lower(w.name)
               ) br ON true
               WHERE lower(w.name) = lower($1)
               FOR UPDATE OF w"#,
        )
        .bind(world_name)
        .bind(input.spawn_coordinates.is_some())
        .fetch_optional(&mut *tx)
        .await?;
        let old_spawn: Option<String> = locked
            .as_ref()
            .and_then(|r| r.get::<Option<String>, _>("spawn_coordinates"));

        if let Some(spawn) = input.spawn_coordinates.as_deref() {
            let rect = locked.as_ref().and_then(|r| {
                let col = |c: &str| r.get::<Option<i32>, _>(c);
                Some((col("min_x")?, col("max_x")?, col("min_y")?, col("max_y")?))
            });
            let (min_x, max_x, min_y, max_y) = rect.ok_or_else(|| {
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

        let categories: Option<Vec<String>> = input
            .categories_provided
            .then(|| input.categories.clone().unwrap_or_default());
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
        .bind(input.skybox_time_provided)
        .bind(has_settings_patch)
        .bind(&input.realm_name_override)
        .bind(input.realm_name_override_provided)
        .bind(&input.preview_wearable_urns)
        .bind(input.preview_wearable_urns_provided)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        self.forget_about(world_name).await;

        Ok((world_settings_from_row(&row), old_spawn))
    }

    pub async fn get_world_manifest(
        &self,
        world_name: &str,
    ) -> Result<Option<WorldManifest>, ApiError> {
        const PARCELS_LIMIT: i64 = 500;

        let (total, parcels, spawn_coordinates): (i64, Vec<String>, Option<String>) =
            sqlx::query_as(
                r#"WITH parcels AS MATERIALIZED (
                       SELECT DISTINCT p AS parcel
                       FROM world_scenes ws, unnest(ws.parcels) AS p
                       WHERE lower(ws.world_name) = lower($1) AND p IS NOT NULL
                   )
                   SELECT (SELECT count(*) FROM parcels),
                          ARRAY(SELECT parcel FROM parcels
                                ORDER BY split_part(parcel, ',', 1)::int,
                                         split_part(parcel, ',', 2)::int LIMIT $2),
                          (SELECT spawn_coordinates FROM worlds
                           WHERE lower(name) = lower($1) LIMIT 1)"#,
            )
            .bind(world_name)
            .bind(PARCELS_LIMIT)
            .fetch_one(&self.pool)
            .await?;

        if total == 0 {
            return Ok(None);
        }

        Ok(Some(WorldManifest {
            parcels,
            spawn_coordinates,
            total,
        }))
    }

    /// `GET /permissions` in one statement: the world row (if any) and its permission
    /// records aggregated as JSON, in the `get_world_permission_records_full` order.
    pub async fn get_world_with_permission_records(
        &self,
        world_name: &str,
    ) -> Result<(Option<WorldRecord>, Vec<PermissionRecordFull>), ApiError> {
        let r = sqlx::query(
            r#"SELECT w.name, w.owner, w.access, w.blocked_since, w.spawn_coordinates,
                      w.skybox_time, w.single_player, w.realm_name_override,
                      w.preview_wearable_urns,
                      COALESCE((
                        SELECT json_agg(json_build_object(
                                 'id', r.id, 'permission_type', r.permission_type,
                                 'address', r.address, 'is_world_wide', r.is_world_wide,
                                 'parcel_count', r.parcel_count)
                               ORDER BY r.address, r.permission_type)
                        FROM (SELECT wp.id, wp.permission_type, wp.address,
                                     count(wpp.parcel) = 0 AS is_world_wide,
                                     count(wpp.parcel) AS parcel_count
                              FROM world_permissions wp
                              LEFT JOIN world_permission_parcels wpp ON wp.id = wpp.permission_id
                              WHERE lower(wp.world_name) = lower($1)
                              GROUP BY wp.id, wp.permission_type, wp.address) r
                      ), '[]'::json) AS records
               FROM (SELECT 1) AS one
               LEFT JOIN worlds w ON lower(w.name) = lower($1)"#,
        )
        .bind(world_name)
        .fetch_one(&self.pool)
        .await?;
        let world = r.get::<Option<String>, _>("name").map(|name| WorldRecord {
            name,
            owner: r.get("owner"),
            access: access_setting(&r),
            blocked_since: r.get("blocked_since"),
            spawn_coordinates: r.get("spawn_coordinates"),
            skybox_time: r.get("skybox_time"),
            single_player: r.get::<Option<bool>, _>("single_player").unwrap_or(false),
            realm_name_override: r.get("realm_name_override"),
            preview_wearable_urns: r.get("preview_wearable_urns"),
        });
        let records: Vec<Value> = r
            .get::<Value, _>("records")
            .as_array()
            .cloned()
            .unwrap_or_default();
        let records = records
            .iter()
            .map(|v| {
                let text = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                Ok(PermissionRecordFull {
                    id: v
                        .get("id")
                        .and_then(|x| x.as_i64())
                        .and_then(|x| i32::try_from(x).ok())
                        .ok_or_else(|| ApiError::internal("permission record id"))?,
                    permission_type: text("permission_type"),
                    address: text("address"),
                    is_world_wide: v
                        .get("is_world_wide")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false),
                    parcel_count: v.get("parcel_count").and_then(|x| x.as_i64()).unwrap_or(0),
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        Ok((world, records))
    }

    /// Ensures the worlds row and stores `access` in one statement. A fresh row lands at
    /// settings_version 1, matching the ensure-then-upsert pair it replaces.
    pub async fn store_access_for_owner(
        &self,
        world_name: &str,
        owner: &str,
        access: &AccessSetting,
    ) -> Result<(), ApiError> {
        sqlx::query(
            r#"INSERT INTO worlds (name, owner, access, settings_version, created_at, updated_at)
               VALUES (lower($1), lower($2), $3::jsonb, 1, now(), now())
               ON CONFLICT (name) DO UPDATE SET access = $3::jsonb,
                 settings_version = worlds.settings_version + 1,
                 updated_at = now()"#,
        )
        .bind(world_name)
        .bind(owner)
        .bind(access_json(access)?)
        .execute(&self.pool)
        .await?;
        self.forget_about(world_name).await;
        Ok(())
    }

    /// One allow-list edit as a guarded UPDATE on the access JSON, replacing the
    /// read-modify-write transaction. Wallets compare case-insensitively (the caller
    /// passes them lowercased), communities exactly; the settings version bumps whenever
    /// the row is an allow-list, as the old unconditional upsert did.
    pub async fn modify_allow_list_access(
        &self,
        world_name: &str,
        edit: AllowListEdit<'_>,
    ) -> Result<AllowListEditOutcome, ApiError> {
        const CUR: &str = r#"WITH cur AS (
                 SELECT access->>'type' = 'allow-list' AS is_allow_list
                 FROM worlds WHERE lower(name) = lower($1)
               ), upd AS ("#;
        const TAIL: &str = r#"
                 RETURNING 1
               )
               SELECT COALESCE((SELECT is_allow_list FROM cur), false) AS is_allow_list,
                      EXISTS (SELECT 1 FROM upd) AS applied"#;
        let (sql, value, cap) = match edit {
            AllowListEdit::AddWallet(w) => (
                format!(
                    r#"{CUR}
                 UPDATE worlds SET
                   access = CASE WHEN EXISTS (
                       SELECT 1 FROM jsonb_array_elements_text(COALESCE(access->'wallets', '[]'::jsonb)) e
                       WHERE lower(e) = $2)
                     THEN access
                     ELSE jsonb_set(access, '{{wallets}}',
                                    COALESCE(access->'wallets', '[]'::jsonb) || to_jsonb($2::text), true)
                     END,
                   settings_version = settings_version + 1,
                   updated_at = now()
                 WHERE lower(name) = lower($1) AND access->>'type' = 'allow-list'
                   AND (EXISTS (
                          SELECT 1 FROM jsonb_array_elements_text(COALESCE(access->'wallets', '[]'::jsonb)) e
                          WHERE lower(e) = $2)
                        OR jsonb_array_length(COALESCE(access->'wallets', '[]'::jsonb)) < $3){TAIL}"#
                ),
                w,
                Some(MAX_ACCESS_WALLETS),
            ),
            AllowListEdit::RemoveWallet(w) => (
                format!(
                    r#"{CUR}
                 UPDATE worlds SET
                   access = jsonb_set(access, '{{wallets}}', COALESCE((
                       SELECT jsonb_agg(e.value ORDER BY e.ordinality)
                       FROM jsonb_array_elements(COALESCE(access->'wallets', '[]'::jsonb))
                            WITH ORDINALITY AS e
                       WHERE lower(e.value #>> '{{}}') <> $2), '[]'::jsonb), true),
                   settings_version = settings_version + 1,
                   updated_at = now()
                 WHERE lower(name) = lower($1) AND access->>'type' = 'allow-list'{TAIL}"#
                ),
                w,
                None,
            ),
            AllowListEdit::AddCommunity(c) => (
                format!(
                    r#"{CUR}
                 UPDATE worlds SET
                   access = CASE WHEN COALESCE(access->'communities', '[]'::jsonb) ? $2
                     THEN access
                     ELSE jsonb_set(access, '{{communities}}',
                                    COALESCE(access->'communities', '[]'::jsonb) || to_jsonb($2::text), true)
                     END,
                   settings_version = settings_version + 1,
                   updated_at = now()
                 WHERE lower(name) = lower($1) AND access->>'type' = 'allow-list'
                   AND (COALESCE(access->'communities', '[]'::jsonb) ? $2
                        OR jsonb_array_length(COALESCE(access->'communities', '[]'::jsonb)) < $3){TAIL}"#
                ),
                c,
                Some(MAX_ACCESS_COMMUNITIES),
            ),
            AllowListEdit::RemoveCommunity(c) => (
                format!(
                    r#"{CUR}
                 UPDATE worlds SET
                   access = jsonb_set(access, '{{communities}}', COALESCE((
                       SELECT jsonb_agg(e.value ORDER BY e.ordinality)
                       FROM jsonb_array_elements(COALESCE(access->'communities', '[]'::jsonb))
                            WITH ORDINALITY AS e
                       WHERE e.value #>> '{{}}' <> $2), '[]'::jsonb), true),
                   settings_version = settings_version + 1,
                   updated_at = now()
                 WHERE lower(name) = lower($1) AND access->>'type' = 'allow-list'{TAIL}"#
                ),
                c,
                None,
            ),
        };
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(world_name)
            .bind(value);
        if let Some(cap) = cap {
            query = query.bind(cap);
        }
        let r = query.fetch_one(&self.pool).await?;
        let outcome = if r.get::<bool, _>("applied") {
            AllowListEditOutcome::Applied
        } else if r.get::<bool, _>("is_allow_list") {
            AllowListEditOutcome::CapExceeded
        } else {
            AllowListEditOutcome::NotAllowList
        };
        if outcome == AllowListEditOutcome::Applied {
            self.forget_about(world_name).await;
        }
        Ok(outcome)
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

    /// Grants world-wide `permission` to `addresses` in one statement: the optional
    /// `ensure_owner` creates the worlds row first (same statement), new rows are inserted
    /// and any parcel scoping the addresses had is cleared. Returns the newly added addresses.
    pub async fn grant_addresses_world_wide_permission(
        &self,
        world_name: &str,
        permission: &str,
        addresses: &[String],
        ensure_owner: Option<&str>,
    ) -> Result<Vec<String>, ApiError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        let inserted = sqlx::query(
            r#"WITH ensured AS (
                 INSERT INTO worlds (name, owner, access, created_at, updated_at)
                 SELECT lower($1), lower($4), $5::jsonb, now(), now()
                 WHERE $4::text IS NOT NULL
                 ON CONFLICT (name) DO NOTHING
               ), added AS (
                 INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
                 SELECT lower($1), $2, addr, now(), now() FROM unnest($3::text[]) AS addr
                 ON CONFLICT (world_name, permission_type, address) DO NOTHING
                 RETURNING address
               ), cleared AS (
                 DELETE FROM world_permission_parcels
                 WHERE permission_id IN (
                   SELECT id FROM world_permissions
                   WHERE lower(world_name) = lower($1)
                     AND permission_type = $2
                     AND address = ANY($3::text[])
                 )
               )
               SELECT address FROM added"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&lowered)
        .bind(ensure_owner)
        .bind(default_access_json())
        .fetch_all(&self.pool)
        .await?;
        Ok(inserted.into_iter().map(|r| r.get("address")).collect())
    }

    /// `POST /permissions/{perm}` in one statement: ensure the worlds row, drop the
    /// addresses no longer listed and add the new ones (existing rows keep their scoping).
    pub async fn replace_world_wide_permission(
        &self,
        world_name: &str,
        owner: &str,
        permission: &str,
        addresses: &[String],
    ) -> Result<(), ApiError> {
        let lowered: Vec<String> = addresses.iter().map(|a| a.to_lowercase()).collect();
        sqlx::query(
            r#"WITH ensured AS (
                 INSERT INTO worlds (name, owner, access, created_at, updated_at)
                 VALUES (lower($1), lower($4), $5::jsonb, now(), now())
                 ON CONFLICT (name) DO NOTHING
               ), removed AS (
                 DELETE FROM world_permissions
                 WHERE lower(world_name) = lower($1)
                   AND permission_type = $2
                   AND NOT (address = ANY($3::text[]))
               ), added AS (
                 INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
                 SELECT lower($1), $2, addr, now(), now() FROM unnest($3::text[]) AS addr
                 ON CONFLICT (world_name, permission_type, address) DO NOTHING
               )
               SELECT 1"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(&lowered)
        .bind(owner)
        .bind(default_access_json())
        .execute(&self.pool)
        .await?;
        Ok(())
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
        let created: bool = sqlx::query_scalar(
            r#"WITH perm AS (
                 INSERT INTO world_permissions (world_name, permission_type, address, created_at, updated_at)
                 VALUES (lower($1), $2, lower($3), now(), now())
                 ON CONFLICT (world_name, permission_type, address) DO UPDATE SET updated_at = now()
                 RETURNING id, (xmax = 0) AS created
               ), added AS (
                 INSERT INTO world_permission_parcels (permission_id, parcel)
                 SELECT perm.id, parcel FROM perm, unnest($4::text[]) AS parcel
                 ON CONFLICT DO NOTHING
               )
               SELECT created FROM perm"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .bind(&canon)
        .fetch_one(&self.pool)
        .await?;
        Ok(created)
    }

    /// `DELETE .../address/{addr}/parcels` in one statement; `false` when the address holds
    /// no such permission (the caller's 400), which the old id lookup answered separately.
    pub async fn remove_parcels_from_permission_by_address(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
        parcels: &[String],
    ) -> Result<bool, ApiError> {
        let canon = canonicalize_parcels(parcels);
        if canon.is_empty() {
            return Ok(self
                .get_address_permission_id(world_name, permission, address)
                .await?
                .is_some());
        }
        let touched: Option<i32> = sqlx::query_scalar(
            r#"WITH perm AS (
                 UPDATE world_permissions SET updated_at = now()
                 WHERE lower(world_name) = lower($1)
                   AND permission_type = $2
                   AND address = lower($3)
                 RETURNING id
               ), removed AS (
                 DELETE FROM world_permission_parcels
                 WHERE permission_id IN (SELECT id FROM perm) AND parcel = ANY($4::text[])
               )
               SELECT id FROM perm"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .bind(&canon)
        .fetch_optional(&self.pool)
        .await?;
        Ok(touched.is_some())
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

    /// `GET .../address/{addr}/parcels` in one statement: `None` when the address holds no
    /// such permission, otherwise the filtered total and the page.
    pub async fn get_parcels_for_permission_by_address(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
        limit: i64,
        offset: i64,
        bbox: Option<(i32, i32, i32, i32)>,
    ) -> Result<Option<(i64, Vec<String>)>, ApiError> {
        let (has_bbox, min_x, max_x, min_y, max_y) = match bbox {
            Some((x1, y1, x2, y2)) => (true, x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2)),
            None => (false, 0, 0, 0, 0),
        };
        let rows = sqlx::query(
            r#"SELECT c.total, p.parcel
               FROM world_permissions wp
               CROSS JOIN LATERAL (
                 SELECT count(*) AS total FROM world_permission_parcels wpp
                 WHERE wpp.permission_id = wp.id
                   AND ($4::bool = false OR (
                      split_part(wpp.parcel, ',', 1)::int BETWEEN $5 AND $6
                      AND split_part(wpp.parcel, ',', 2)::int BETWEEN $7 AND $8))
               ) c
               LEFT JOIN LATERAL (
                 SELECT wpp.parcel FROM world_permission_parcels wpp
                 WHERE wpp.permission_id = wp.id
                   AND ($4::bool = false OR (
                      split_part(wpp.parcel, ',', 1)::int BETWEEN $5 AND $6
                      AND split_part(wpp.parcel, ',', 2)::int BETWEEN $7 AND $8))
                 ORDER BY wpp.parcel
                 LIMIT $9 OFFSET $10
               ) p ON true
               WHERE lower(wp.world_name) = lower($1)
                 AND wp.permission_type = $2
                 AND wp.address = lower($3)
               ORDER BY p.parcel"#,
        )
        .bind(world_name)
        .bind(permission)
        .bind(address)
        .bind(has_bbox)
        .bind(min_x)
        .bind(max_x)
        .bind(min_y)
        .bind(max_y)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let total: i64 = first.get("total");
        let parcels = rows
            .iter()
            .filter_map(|r| r.get::<Option<String>, _>("parcel"))
            .collect();
        Ok(Some((total, parcels)))
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
        let rows = sqlx::query(
            r#"SELECT wp.address, count(*) OVER () AS total FROM world_permissions wp
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
        let total: i64 = match rows.first() {
            Some(r) => r.get("total"),
            None if offset > 0 => {
                sqlx::query_scalar(
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
                .await?
            }
            None => 0,
        };
        Ok((total, rows.into_iter().map(|r| r.get("address")).collect()))
    }

    /// Whether `address` holds a deployment grant covering every parcel in `required`
    /// (world-wide, or parcel-scoped over the full set). Both sides are canonical
    /// "x,y" strings: stored parcels are canonicalized on insert and callers pass
    /// `canon_pointer` output, so equality here is the old per-record set comparison.
    pub async fn has_deployment_permission_covering(
        &self,
        world_name: &str,
        address: &str,
        required: &[String],
    ) -> Result<bool, ApiError> {
        Ok(sqlx::query_scalar(
            r#"SELECT EXISTS (
                 SELECT 1 FROM world_permissions wp
                 LEFT JOIN world_permission_parcels wpp ON wpp.permission_id = wp.id
                 WHERE lower(wp.world_name) = lower($1)
                   AND wp.permission_type = 'deployment'
                   AND wp.address = lower($2)
                 GROUP BY wp.id
                 HAVING count(wpp.parcel) = 0
                     OR count(DISTINCT wpp.parcel) FILTER (WHERE wpp.parcel = ANY($3::text[]))
                        = cardinality($3::text[])
               )"#,
        )
        .bind(world_name)
        .bind(address)
        .bind(required)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn has_world_wide_permission(
        &self,
        world_name: &str,
        permission: &str,
        address: &str,
    ) -> Result<bool, ApiError> {
        Ok(sqlx::query_scalar(
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
        .await?)
    }

    pub async fn store_access(
        &self,
        world_name: &str,
        access: &AccessSetting,
    ) -> Result<(), ApiError> {
        upsert_world_access(&self.pool, world_name, &access_json(access)?).await?;
        self.forget_about(world_name).await;
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
        let current = row.as_ref().map(access_setting).unwrap_or_default();

        let updated = modifier(current)?;
        upsert_world_access(&mut *tx, world_name, &access_json(&updated)?).await?;
        tx.commit().await?;
        self.forget_about(world_name).await;
        Ok(updated)
    }
}
