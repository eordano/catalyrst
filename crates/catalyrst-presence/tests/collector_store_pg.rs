//! The snapshot writer lands every per-table row set in one `unnest` INSERT, and the
//! `/current*` memo serves the collector's latest commit. DB-gated on
//! `CATALYRST_TEST_PG`; skips cleanly when unset.

use std::str::FromStr;

use catalyrst_presence::ports::collector::{store_snapshot, SnapshotData};
use catalyrst_presence::ports::model::{HotScene, Island, Peer};
use catalyrst_presence::ports::queries::QueriesComponent;
use serde_json::json;
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;

async fn scratch_pool() -> Option<(PgPool, String)> {
    let url = std::env::var("CATALYRST_TEST_PG").ok()?;
    let schema = format!("presence_rt_{}", std::process::id());
    let admin = PgPool::connect(&url).await.expect("connect scratch pg");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let opts = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPool::connect_with(opts).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_presence.sql"))
        .execute(&pool)
        .await
        .expect("apply presence migration");
    Some((pool, schema))
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(pool)
    .await
    .unwrap();
}

fn peer(address: &str, parcel: Option<(i32, i32)>) -> Peer {
    Peer {
        address: address.into(),
        parcel_x: parcel.map(|p| p.0),
        parcel_y: parcel.map(|p| p.1),
        position_x: parcel.map(|p| p.0 as f64 * 16.0),
        position_y: None,
        position_z: parcel.map(|p| p.1 as f64 * 16.0),
        last_ping: Some(42),
    }
}

fn sample() -> SnapshotData {
    SnapshotData {
        realm: "main".into(),
        peers: vec![
            peer("0xaa", Some((1, -2))),
            peer("0xbb", None),
            peer("0xaa", Some((3, 4))),
        ],
        islands: vec![Island {
            island_id: "I1".into(),
            peer_count: 2,
            max_peers: None,
            center_x: Some(1.5),
            center_y: None,
            center_z: Some(-3.25),
            radius: Some(100.0),
        }],
        hot_scenes: vec![
            HotScene {
                scene_id: "s1".into(),
                name: Some("Plaza".into()),
                base_x: Some(1),
                base_y: Some(-2),
                users_count: Some(2),
                parcel_count: 4,
                creator: None,
                description: Some("d".into()),
            },
            HotScene {
                scene_id: "s2".into(),
                name: None,
                base_x: None,
                base_y: None,
                users_count: None,
                parcel_count: 0,
                creator: Some("c".into()),
                description: None,
            },
        ],
        occupancy: vec![
            (
                "1,-2".into(),
                Some("Plaza".into()),
                vec!["0xaa".into(), "0xbb".into()],
            ),
            ("1,-2".into(), Some("Dup".into()), vec![]),
            ("7,7".into(), None, vec![]),
        ],
        world_rows: vec![
            ("alice.eth".into(), vec!["0xcc".into()], 1),
            ("empty.eth".into(), vec![], 3),
        ],
        worlds_live_total: Some(4),
    }
}

#[tokio::test]
async fn store_snapshot_writes_every_table_and_memo_serves_latest() {
    let Some((pool, schema)) = scratch_pool().await else {
        eprintln!("CATALYRST_TEST_PG unset; skipping");
        return;
    };
    let queries = QueriesComponent::new(pool.clone());
    assert!(queries.current_bundle().await.unwrap().current.is_none());

    let id = store_snapshot(&pool, &sample()).await.unwrap();

    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM peer_snapshots WHERE snapshot_id = $1), \
                (SELECT count(*) FROM island_snapshots WHERE snapshot_id = $1), \
                (SELECT count(*) FROM hot_scene_snapshots WHERE snapshot_id = $1), \
                (SELECT count(*) FROM scene_occupancy WHERE snapshot_id = $1), \
                (SELECT count(*) FROM world_membership WHERE snapshot_id = $1)",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (2, 1, 2, 2, 2),
        "ON CONFLICT DO NOTHING keeps first-wins per key"
    );

    let (parcel_x, position_z, last_ping): (Option<i32>, Option<f64>, Option<i64>) =
        sqlx::query_as(
            "SELECT parcel_x, position_z, last_ping FROM peer_snapshots \
             WHERE snapshot_id = $1 AND address = '0xaa'",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (parcel_x, position_z, last_ping),
        (Some(1), Some(-32.0), Some(42))
    );

    let (name, addresses, count, realm): (Option<String>, serde_json::Value, i32, String) =
        sqlx::query_as(
            "SELECT scene_name, addresses, count, realm FROM scene_occupancy \
             WHERE snapshot_id = $1 AND pointer = '1,-2'",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name.as_deref(), Some("Plaza"));
    assert_eq!(addresses, json!(["0xaa", "0xbb"]));
    assert_eq!((count, realm.as_str()), (2, "main"));

    let (w_count, live): (i32, Option<i32>) = sqlx::query_as(
        "SELECT count, live_users FROM world_membership \
         WHERE snapshot_id = $1 AND world_name = 'empty.eth'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((w_count, live), (0, Some(3)));

    let snap: (i32, i32, i32, i32, i32, Option<i32>) = sqlx::query_as(
        "SELECT peers_count, scenes_polled, scene_users_total, worlds_polled, \
                active_worlds, worlds_live_total FROM snapshots WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(snap, (3, 3, 2, 2, 1, Some(4)));

    // Still the pre-commit memo until the collector refreshes it.
    assert!(queries.current_bundle().await.unwrap().current.is_none());
    queries.refresh_current().await.unwrap();
    let bundle = queries.current_bundle().await.unwrap();
    assert_eq!(bundle.current.as_ref().map(|c| c.snapshot_id), Some(id));
    assert_eq!(bundle.scenes.len(), 2);
    assert_eq!(bundle.scenes[0].pointer, "1,-2");
    assert_eq!(bundle.worlds.len(), 2);
    assert_eq!(bundle.worlds[0].world_name, "alice.eth");

    drop_schema(&pool, &schema).await;
}
