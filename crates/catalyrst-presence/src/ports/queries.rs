use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::postgres::PgPool;
use sqlx::Row;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Snapshots land every collector tick (300 s by default), so the `/current*` trio is
/// served from one memo that the collector refreshes on commit; the TTL only covers a
/// `serve` process whose snapshots are written elsewhere.
const CURRENT_TTL: Duration = Duration::from_secs(30);

type CurrentCache = Arc<Mutex<Option<(Instant, Arc<CurrentBundle>)>>>;

#[derive(Clone)]
pub struct QueriesComponent {
    pool: PgPool,
    current: CurrentCache,
}

#[derive(Debug, Clone, Default)]
pub struct CurrentBundle {
    pub current: Option<CurrentSnapshot>,
    pub scenes: Vec<SceneOccupancyRow>,
    pub worlds: Vec<WorldHeadcountRow>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "presence/"))]
pub struct CurrentSnapshot {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub snapshot_id: i64,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub taken_at: DateTime<Utc>,
    pub peers_count: i32,
    pub islands_count: i32,
    pub hot_scenes_count: i32,
    pub scenes_polled: i32,
    pub scene_users_total: i32,
    pub worlds_polled: i32,
    pub active_worlds: i32,
    pub world_users_total: i32,
    pub worlds_live_total: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "presence/"))]
pub struct SceneOccupancyRow {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub taken_at: DateTime<Utc>,
    pub pointer: String,
    pub scene_name: Option<String>,
    pub realm: String,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "presence/"))]
pub struct WorldHeadcountRow {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub taken_at: DateTime<Utc>,
    pub world_name: String,
    pub count: i32,
    pub live_users: Option<i32>,
}

impl QueriesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            current: Arc::new(Mutex::new(None)),
        }
    }

    /// The latest snapshot with its scene and world rows, memoized for `CURRENT_TTL`;
    /// concurrent misses coalesce onto one load.
    pub async fn current_bundle(&self) -> Result<Arc<CurrentBundle>, sqlx::Error> {
        let mut slot = self.current.lock().await;
        if let Some((at, bundle)) = slot.as_ref() {
            if at.elapsed() < CURRENT_TTL {
                return Ok(bundle.clone());
            }
        }
        let bundle = Arc::new(self.load_current_bundle().await?);
        *slot = Some((Instant::now(), bundle.clone()));
        Ok(bundle)
    }

    pub async fn refresh_current(&self) -> Result<(), sqlx::Error> {
        let bundle = Arc::new(self.load_current_bundle().await?);
        *self.current.lock().await = Some((Instant::now(), bundle));
        Ok(())
    }

    async fn load_current_bundle(&self) -> Result<CurrentBundle, sqlx::Error> {
        let (current, scenes, worlds) =
            tokio::try_join!(self.current(), self.current_scenes(), self.current_worlds())?;
        Ok(CurrentBundle {
            current,
            scenes,
            worlds,
        })
    }

    pub async fn current(&self) -> Result<Option<CurrentSnapshot>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, taken_at, peers_count, islands_count, hot_scenes_count, \
                    scenes_polled, scene_users_total, worlds_polled, active_worlds, \
                    world_users_total, worlds_live_total \
             FROM snapshots ORDER BY taken_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| CurrentSnapshot {
            snapshot_id: r.get("id"),
            taken_at: r.get("taken_at"),
            peers_count: r.get("peers_count"),
            islands_count: r.get("islands_count"),
            hot_scenes_count: r.get("hot_scenes_count"),
            scenes_polled: r.get("scenes_polled"),
            scene_users_total: r.get("scene_users_total"),
            worlds_polled: r.get("worlds_polled"),
            active_worlds: r.get("active_worlds"),
            world_users_total: r.get("world_users_total"),
            worlds_live_total: r.get("worlds_live_total"),
        }))
    }

    pub async fn scene_history(
        &self,
        pointer: &str,
        limit: i64,
    ) -> Result<Vec<SceneOccupancyRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT taken_at, pointer, scene_name, realm, count \
             FROM scene_occupancy WHERE pointer = $1 \
             ORDER BY taken_at DESC LIMIT $2",
        )
        .bind(pointer)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SceneOccupancyRow {
                taken_at: r.get("taken_at"),
                pointer: r.get("pointer"),
                scene_name: r.get("scene_name"),
                realm: r.get("realm"),
                count: r.get("count"),
            })
            .collect())
    }

    pub async fn current_scenes(&self) -> Result<Vec<SceneOccupancyRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT taken_at, pointer, scene_name, realm, count \
             FROM scene_occupancy \
             WHERE snapshot_id = (SELECT id FROM snapshots ORDER BY taken_at DESC LIMIT 1) \
             ORDER BY count DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SceneOccupancyRow {
                taken_at: r.get("taken_at"),
                pointer: r.get("pointer"),
                scene_name: r.get("scene_name"),
                realm: r.get("realm"),
                count: r.get("count"),
            })
            .collect())
    }

    pub async fn world_history(
        &self,
        world: &str,
        limit: i64,
    ) -> Result<Vec<WorldHeadcountRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT taken_at, world_name, count, live_users \
             FROM world_membership WHERE world_name = $1 \
             ORDER BY taken_at DESC LIMIT $2",
        )
        .bind(world)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| WorldHeadcountRow {
                taken_at: r.get("taken_at"),
                world_name: r.get("world_name"),
                count: r.get("count"),
                live_users: r.get("live_users"),
            })
            .collect())
    }

    pub async fn current_worlds(&self) -> Result<Vec<WorldHeadcountRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT taken_at, world_name, count, live_users \
             FROM world_membership \
             WHERE snapshot_id = (SELECT id FROM snapshots ORDER BY taken_at DESC LIMIT 1) \
             ORDER BY count DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| WorldHeadcountRow {
                taken_at: r.get("taken_at"),
                world_name: r.get("world_name"),
                count: r.get("count"),
                live_users: r.get("live_users"),
            })
            .collect())
    }
}
