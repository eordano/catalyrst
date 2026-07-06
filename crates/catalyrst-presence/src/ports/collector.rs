use anyhow::{Context, Result};
use serde_json::json;
use sqlx::postgres::PgPool;

use super::model;
use super::upstream::UpstreamClient;

#[derive(Debug, Clone)]
pub struct SnapshotSummary {
    pub snapshot_id: i64,
    pub peers: i32,
    pub islands: i32,
    pub hot_scenes: i32,
    pub scenes_polled: i32,
    pub scene_users: i32,
    pub worlds_polled: i32,
    pub active_worlds: i32,
    pub world_users: i32,
}

#[derive(Clone)]
pub struct Collector {
    pool: PgPool,
    client: UpstreamClient,
}

impl Collector {
    pub fn new(pool: PgPool, client: UpstreamClient) -> Self {
        Self { pool, client }
    }

    pub async fn snapshot(&self) -> Result<SnapshotSummary> {
        let peers = model::parse_peers(&self.client.peers().await?);
        let islands = model::parse_islands(&self.client.islands().await?);
        let hot_scenes_raw = self.client.hot_scenes().await?;
        let hot_scenes = model::parse_hot_scenes(&hot_scenes_raw);

        let mut occupancy: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
        let mut scene_users_total: i32 = 0;
        for scene in &hot_scenes {
            let Some(pointer) = model::hot_scene_pointer(scene) else {
                continue;
            };
            let Some(resp) = self.client.scene_participants(&pointer).await? else {
                continue;
            };
            let addresses = model::parse_participants(&resp);
            scene_users_total += addresses.len() as i32;
            occupancy.push((pointer, scene.name.clone(), addresses));
        }

        let (active_worlds, worlds_live_total) = match self.client.worlds_live_data().await? {
            Some(v) => model::parse_active_worlds(&v),
            None => (Vec::new(), None),
        };
        let mut world_rows: Vec<(String, Vec<String>, i32)> = Vec::new();
        let mut world_users_total: i32 = 0;
        let mut worlds_with_members = 0i32;
        for w in &active_worlds {
            let addresses = match self.client.world_participants(&w.world_name).await? {
                Some(resp) => model::parse_participants(&resp),
                None => Vec::new(),
            };
            if !addresses.is_empty() {
                worlds_with_members += 1;
            }
            world_users_total += addresses.len() as i32;
            world_rows.push((w.world_name.clone(), addresses, w.users));
        }

        let realm = self.client.genesis_realm().to_string();
        let mut tx = self.pool.begin().await.context("begin tx")?;

        let snapshot_id: i64 = sqlx::query_scalar(
            "INSERT INTO snapshots \
                (peers_count, islands_count, hot_scenes_count, \
                 scenes_polled, scene_users_total, \
                 worlds_polled, active_worlds, world_users_total, worlds_live_total) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
        )
        .bind(peers.len() as i32)
        .bind(islands.len() as i32)
        .bind(hot_scenes.len() as i32)
        .bind(occupancy.len() as i32)
        .bind(scene_users_total)
        .bind(world_rows.len() as i32)
        .bind(worlds_with_members)
        .bind(world_users_total)
        .bind(worlds_live_total)
        .fetch_one(&mut *tx)
        .await
        .context("insert snapshot")?;

        for p in &peers {
            sqlx::query(
                "INSERT INTO peer_snapshots \
                    (snapshot_id, address, parcel_x, parcel_y, position_x, position_y, position_z, last_ping) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            )
            .bind(snapshot_id)
            .bind(&p.address)
            .bind(p.parcel_x)
            .bind(p.parcel_y)
            .bind(p.position_x)
            .bind(p.position_y)
            .bind(p.position_z)
            .bind(p.last_ping)
            .execute(&mut *tx)
            .await
            .context("insert peer")?;
        }

        for i in &islands {
            sqlx::query(
                "INSERT INTO island_snapshots \
                    (snapshot_id, island_id, peer_count, max_peers, center_x, center_y, center_z, radius) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            )
            .bind(snapshot_id)
            .bind(&i.island_id)
            .bind(i.peer_count)
            .bind(i.max_peers)
            .bind(i.center_x)
            .bind(i.center_y)
            .bind(i.center_z)
            .bind(i.radius)
            .execute(&mut *tx)
            .await
            .context("insert island")?;
        }

        for s in &hot_scenes {
            sqlx::query(
                "INSERT INTO hot_scene_snapshots \
                    (snapshot_id, scene_id, name, base_x, base_y, users_count, parcel_count, creator, description) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            )
            .bind(snapshot_id)
            .bind(&s.scene_id)
            .bind(&s.name)
            .bind(s.base_x)
            .bind(s.base_y)
            .bind(s.users_count)
            .bind(s.parcel_count)
            .bind(&s.creator)
            .bind(&s.description)
            .execute(&mut *tx)
            .await
            .context("insert hot scene")?;
        }

        for (pointer, scene_name, addresses) in &occupancy {
            sqlx::query(
                "INSERT INTO scene_occupancy \
                    (snapshot_id, pointer, scene_name, realm, addresses, count) \
                 VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
            )
            .bind(snapshot_id)
            .bind(pointer)
            .bind(scene_name)
            .bind(&realm)
            .bind(json!(addresses))
            .bind(addresses.len() as i32)
            .execute(&mut *tx)
            .await
            .context("insert scene occupancy")?;
        }

        for (world_name, addresses, live_users) in &world_rows {
            sqlx::query(
                "INSERT INTO world_membership \
                    (snapshot_id, taken_at, world_name, addresses, count, live_users) \
                 VALUES ($1, now(), $2, $3, $4, $5) ON CONFLICT DO NOTHING",
            )
            .bind(snapshot_id)
            .bind(world_name)
            .bind(json!(addresses))
            .bind(addresses.len() as i32)
            .bind(live_users)
            .execute(&mut *tx)
            .await
            .context("insert world membership")?;
        }

        tx.commit().await.context("commit snapshot")?;

        Ok(SnapshotSummary {
            snapshot_id,
            peers: peers.len() as i32,
            islands: islands.len() as i32,
            hot_scenes: hot_scenes.len() as i32,
            scenes_polled: occupancy.len() as i32,
            scene_users: scene_users_total,
            worlds_polled: world_rows.len() as i32,
            active_worlds: worlds_with_members,
            world_users: world_users_total,
        })
    }

    pub async fn aggregate_day(&self, date: chrono::NaiveDate) -> Result<()> {
        sqlx::query(
            "INSERT INTO daily_stats \
                (date, snapshots_taken, peak_peers, avg_peers, \
                 peak_hot_scene_users, peak_scene_users, peak_world_users) \
             SELECT $1::date, \
                    COUNT(*)::int, \
                    COALESCE(MAX(peers_count), 0)::int, \
                    COALESCE(AVG(peers_count), 0)::double precision, \
                    COALESCE((SELECT MAX(hs.users_count) FROM hot_scene_snapshots hs \
                              JOIN snapshots s2 ON s2.id = hs.snapshot_id \
                              WHERE s2.taken_at >= $1::date AND s2.taken_at < ($1::date + 1)), 0)::int, \
                    COALESCE((SELECT MAX(so.count) FROM scene_occupancy so \
                              JOIN snapshots s3 ON s3.id = so.snapshot_id \
                              WHERE s3.taken_at >= $1::date AND s3.taken_at < ($1::date + 1)), 0)::int, \
                    COALESCE((SELECT MAX(wm.count) FROM world_membership wm \
                              JOIN snapshots s4 ON s4.id = wm.snapshot_id \
                              WHERE s4.taken_at >= $1::date AND s4.taken_at < ($1::date + 1)), 0)::int \
             FROM snapshots s \
             WHERE s.taken_at >= $1::date AND s.taken_at < ($1::date + 1) \
             ON CONFLICT (date) DO UPDATE SET \
                snapshots_taken = EXCLUDED.snapshots_taken, \
                peak_peers = EXCLUDED.peak_peers, \
                avg_peers = EXCLUDED.avg_peers, \
                peak_hot_scene_users = EXCLUDED.peak_hot_scene_users, \
                peak_scene_users = EXCLUDED.peak_scene_users, \
                peak_world_users = EXCLUDED.peak_world_users",
        )
        .bind(date)
        .execute(&self.pool)
        .await
        .context("aggregate daily_stats")?;
        Ok(())
    }
}
