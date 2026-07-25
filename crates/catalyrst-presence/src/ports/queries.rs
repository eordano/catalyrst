use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::postgres::PgPool;
use sqlx::Row;

#[derive(Clone)]
pub struct QueriesComponent {
    pool: PgPool,
}

#[derive(Debug, Serialize)]
pub struct CurrentSnapshot {
    pub snapshot_id: i64,
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

#[derive(Debug, Serialize)]
pub struct SceneOccupancyRow {
    pub taken_at: DateTime<Utc>,
    pub pointer: String,
    pub scene_name: Option<String>,
    pub realm: String,
    pub count: i32,
}

#[derive(Debug, Serialize)]
pub struct WorldHeadcountRow {
    pub taken_at: DateTime<Utc>,
    pub world_name: String,
    pub count: i32,
    pub live_users: Option<i32>,
}

impl QueriesComponent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
