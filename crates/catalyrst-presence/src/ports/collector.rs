use std::future::Future;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::postgres::PgPool;
use tokio::task::JoinSet;

use super::model::{self, HotScene, Island, Peer};
use super::upstream::UpstreamClient;

const PARTICIPANT_FANOUT: usize = 8;

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

/// One collection pass, fully fetched and parsed; `store_snapshot` writes it.
#[derive(Debug, Clone, Default)]
pub struct SnapshotData {
    pub realm: String,
    pub peers: Vec<Peer>,
    pub islands: Vec<Island>,
    pub hot_scenes: Vec<HotScene>,
    pub occupancy: Vec<(String, Option<String>, Vec<String>)>,
    pub world_rows: Vec<(String, Vec<String>, i32)>,
    pub worlds_live_total: Option<i32>,
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
        let data = self.fetch().await?;
        let snapshot_id = store_snapshot(&self.pool, &data).await?;
        Ok(summarize(snapshot_id, &data))
    }

    async fn fetch(&self) -> Result<SnapshotData> {
        let (peers_raw, islands_raw, hot_scenes_raw, live) = tokio::try_join!(
            self.client.peers(),
            self.client.islands(),
            self.client.hot_scenes(),
            self.client.worlds_live_data(),
        )?;
        let peers = model::parse_peers(&peers_raw);
        let islands = model::parse_islands(&islands_raw);
        let hot_scenes = model::parse_hot_scenes(&hot_scenes_raw);
        let (active_worlds, worlds_live_total) = match live {
            Some(v) => model::parse_active_worlds(&v),
            None => (Vec::new(), None),
        };

        let pointers: Vec<(String, Option<String>)> = hot_scenes
            .iter()
            .filter_map(|s| model::hot_scene_pointer(s).map(|p| (p, s.name.clone())))
            .collect();
        let targets: Vec<(bool, String)> = pointers
            .iter()
            .map(|(p, _)| (false, p.clone()))
            .chain(active_worlds.iter().map(|w| (true, w.world_name.clone())))
            .collect();
        let responses = fan_out(targets.len(), PARTICIPANT_FANOUT, |i| {
            let client = self.client.clone();
            let (is_world, key) = targets[i].clone();
            async move {
                if is_world {
                    client.world_participants(&key).await
                } else {
                    client.scene_participants(&key).await
                }
            }
        })
        .await?;
        let (scene_resps, world_resps) = responses.split_at(pointers.len());

        let occupancy = pointers
            .into_iter()
            .zip(scene_resps)
            .filter_map(|((pointer, name), resp)| {
                resp.as_ref()
                    .map(|r| (pointer, name, model::parse_participants(r)))
            })
            .collect();
        let world_rows = active_worlds
            .iter()
            .zip(world_resps)
            .map(|(w, resp)| {
                let addresses = resp
                    .as_ref()
                    .map(model::parse_participants)
                    .unwrap_or_default();
                (w.world_name.clone(), addresses, w.users)
            })
            .collect();

        Ok(SnapshotData {
            realm: self.client.genesis_realm().to_string(),
            peers,
            islands,
            hot_scenes,
            occupancy,
            world_rows,
            worlds_live_total,
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

pub fn summarize(snapshot_id: i64, data: &SnapshotData) -> SnapshotSummary {
    SnapshotSummary {
        snapshot_id,
        peers: data.peers.len() as i32,
        islands: data.islands.len() as i32,
        hot_scenes: data.hot_scenes.len() as i32,
        scenes_polled: data.occupancy.len() as i32,
        scene_users: data.occupancy.iter().map(|(_, _, a)| a.len() as i32).sum(),
        worlds_polled: data.world_rows.len() as i32,
        active_worlds: data
            .world_rows
            .iter()
            .filter(|(_, a, _)| !a.is_empty())
            .count() as i32,
        world_users: data.world_rows.iter().map(|(_, a, _)| a.len() as i32).sum(),
    }
}

/// Runs `make(i)` for `0..n` with at most `limit` in flight, preserving index order;
/// the first error aborts the rest.
async fn fan_out<T, F, Fut>(n: usize, limit: usize, make: F) -> Result<Vec<T>>
where
    T: Send + 'static,
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<T>> + Send + 'static,
{
    let mut set = JoinSet::new();
    let mut out: Vec<Option<T>> = (0..n).map(|_| None).collect();
    let mut next = 0;
    loop {
        while next < n && set.len() < limit.max(1) {
            let i = next;
            next += 1;
            let fut = make(i);
            set.spawn(async move { (i, fut.await) });
        }
        match set.join_next().await {
            Some(Ok((i, Ok(v)))) => out[i] = Some(v),
            Some(Ok((_, Err(e)))) => return Err(e),
            Some(Err(e)) => return Err(e).context("participant fetch task"),
            None => break,
        }
    }
    Ok(out.into_iter().flatten().collect())
}

pub async fn store_snapshot(pool: &PgPool, data: &SnapshotData) -> Result<i64> {
    let summary = summarize(0, data);
    let mut tx = pool.begin().await.context("begin tx")?;

    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO snapshots \
            (peers_count, islands_count, hot_scenes_count, \
             scenes_polled, scene_users_total, \
             worlds_polled, active_worlds, world_users_total, worlds_live_total) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(summary.peers)
    .bind(summary.islands)
    .bind(summary.hot_scenes)
    .bind(summary.scenes_polled)
    .bind(summary.scene_users)
    .bind(summary.worlds_polled)
    .bind(summary.active_worlds)
    .bind(summary.world_users)
    .bind(data.worlds_live_total)
    .fetch_one(&mut *tx)
    .await
    .context("insert snapshot")?;

    if !data.peers.is_empty() {
        let p = &data.peers;
        sqlx::query(
            "INSERT INTO peer_snapshots \
                (snapshot_id, address, parcel_x, parcel_y, position_x, position_y, position_z, last_ping) \
             SELECT $1, * FROM unnest($2::text[], $3::int4[], $4::int4[], \
                                      $5::float8[], $6::float8[], $7::float8[], $8::int8[]) \
             ON CONFLICT DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(p.iter().map(|x| x.address.clone()).collect::<Vec<String>>())
        .bind(p.iter().map(|x| x.parcel_x).collect::<Vec<Option<i32>>>())
        .bind(p.iter().map(|x| x.parcel_y).collect::<Vec<Option<i32>>>())
        .bind(p.iter().map(|x| x.position_x).collect::<Vec<Option<f64>>>())
        .bind(p.iter().map(|x| x.position_y).collect::<Vec<Option<f64>>>())
        .bind(p.iter().map(|x| x.position_z).collect::<Vec<Option<f64>>>())
        .bind(p.iter().map(|x| x.last_ping).collect::<Vec<Option<i64>>>())
        .execute(&mut *tx)
        .await
        .context("insert peers")?;
    }

    if !data.islands.is_empty() {
        let i = &data.islands;
        sqlx::query(
            "INSERT INTO island_snapshots \
                (snapshot_id, island_id, peer_count, max_peers, center_x, center_y, center_z, radius) \
             SELECT $1, * FROM unnest($2::text[], $3::int4[], $4::int4[], \
                                      $5::float8[], $6::float8[], $7::float8[], $8::float8[]) \
             ON CONFLICT DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(i.iter().map(|x| x.island_id.clone()).collect::<Vec<String>>())
        .bind(i.iter().map(|x| x.peer_count).collect::<Vec<i32>>())
        .bind(i.iter().map(|x| x.max_peers).collect::<Vec<Option<i32>>>())
        .bind(i.iter().map(|x| x.center_x).collect::<Vec<Option<f64>>>())
        .bind(i.iter().map(|x| x.center_y).collect::<Vec<Option<f64>>>())
        .bind(i.iter().map(|x| x.center_z).collect::<Vec<Option<f64>>>())
        .bind(i.iter().map(|x| x.radius).collect::<Vec<Option<f64>>>())
        .execute(&mut *tx)
        .await
        .context("insert islands")?;
    }

    if !data.hot_scenes.is_empty() {
        let s = &data.hot_scenes;
        sqlx::query(
            "INSERT INTO hot_scene_snapshots \
                (snapshot_id, scene_id, name, base_x, base_y, users_count, parcel_count, creator, description) \
             SELECT $1, * FROM unnest($2::text[], $3::text[], $4::int4[], $5::int4[], \
                                      $6::int4[], $7::int4[], $8::text[], $9::text[]) \
             ON CONFLICT DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(s.iter().map(|x| x.scene_id.clone()).collect::<Vec<String>>())
        .bind(s.iter().map(|x| x.name.clone()).collect::<Vec<Option<String>>>())
        .bind(s.iter().map(|x| x.base_x).collect::<Vec<Option<i32>>>())
        .bind(s.iter().map(|x| x.base_y).collect::<Vec<Option<i32>>>())
        .bind(s.iter().map(|x| x.users_count).collect::<Vec<Option<i32>>>())
        .bind(s.iter().map(|x| x.parcel_count).collect::<Vec<i32>>())
        .bind(s.iter().map(|x| x.creator.clone()).collect::<Vec<Option<String>>>())
        .bind(s.iter().map(|x| x.description.clone()).collect::<Vec<Option<String>>>())
        .execute(&mut *tx)
        .await
        .context("insert hot scenes")?;
    }

    if !data.occupancy.is_empty() {
        let o = &data.occupancy;
        sqlx::query(
            "INSERT INTO scene_occupancy \
                (snapshot_id, pointer, scene_name, realm, addresses, count) \
             SELECT $1, t.pointer, t.scene_name, $3, t.addresses, t.count \
             FROM unnest($2::text[], $4::text[], $5::jsonb[], $6::int4[]) \
                  AS t(pointer, scene_name, addresses, count) \
             ON CONFLICT DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(o.iter().map(|(p, _, _)| p.clone()).collect::<Vec<String>>())
        .bind(&data.realm)
        .bind(
            o.iter()
                .map(|(_, n, _)| n.clone())
                .collect::<Vec<Option<String>>>(),
        )
        .bind(o.iter().map(|(_, _, a)| json!(a)).collect::<Vec<Value>>())
        .bind(
            o.iter()
                .map(|(_, _, a)| a.len() as i32)
                .collect::<Vec<i32>>(),
        )
        .execute(&mut *tx)
        .await
        .context("insert scene occupancy")?;
    }

    if !data.world_rows.is_empty() {
        let w = &data.world_rows;
        sqlx::query(
            "INSERT INTO world_membership \
                (snapshot_id, taken_at, world_name, addresses, count, live_users) \
             SELECT $1, now(), t.world_name, t.addresses, t.count, t.live_users \
             FROM unnest($2::text[], $3::jsonb[], $4::int4[], $5::int4[]) \
                  AS t(world_name, addresses, count, live_users) \
             ON CONFLICT DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(w.iter().map(|(n, _, _)| n.clone()).collect::<Vec<String>>())
        .bind(w.iter().map(|(_, a, _)| json!(a)).collect::<Vec<Value>>())
        .bind(
            w.iter()
                .map(|(_, a, _)| a.len() as i32)
                .collect::<Vec<i32>>(),
        )
        .bind(w.iter().map(|(_, _, l)| *l).collect::<Vec<i32>>())
        .execute(&mut *tx)
        .await
        .context("insert world membership")?;
    }

    tx.commit().await.context("commit snapshot")?;
    Ok(snapshot_id)
}
