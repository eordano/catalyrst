use axum::{routing::get, Json, Router};
use catalyrst_contract_gate::pg::ScratchDb;
use catalyrst_presence::{
    config::Config,
    ports::{collector::Collector, upstream::UpstreamClient},
};
use serde_json::{json, Value};

#[tokio::test]
async fn snapshot_batches_preserve_duplicates_nulls_and_atomicity() {
    let Some(db) = ScratchDb::create("CATALYRST_PRESENCE_TEST_PG", "presence_batch").await else {
        return;
    };
    sqlx::raw_sql(include_str!("../migrations/0001_presence.sql"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE insert_calls (table_name text); CREATE FUNCTION count_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO insert_calls VALUES (TG_TABLE_NAME); RETURN NULL; END $$;")
        .execute(&db.pool).await.unwrap();
    for table in [
        "snapshots",
        "peer_snapshots",
        "island_snapshots",
        "hot_scene_snapshots",
        "scene_occupancy",
        "world_membership",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TRIGGER count_insert AFTER INSERT ON {table} FOR EACH STATEMENT EXECUTE FUNCTION count_insert()"))).execute(&db.pool).await.unwrap();
    }
    let mut peers: Vec<Value> = (0..513)
        .map(|i| json!({"address":format!("peer-{i}")}))
        .collect();
    peers.push(json!({"address":"peer-0", "parcel":[99,99]}));
    let app = Router::new()
        .route("/peers", get(move || { let peers=peers.clone(); async move { Json(peers) } }))
        .route("/islands", get(|| async { Json(json!([{"id":"island-1","peers":["peer-0"]}])) }))
        .route("/hot-scenes", get(|| async { Json(json!([{"id":"scene-1","name":"tour","baseCoords":[0,0],"parcels":[[0,0]],"usersTotalCount":2}])) }))
        .route("/scene-participants", get(|| async { Json(json!(["0xAbC","0xabc","0xDef"])) }))
        .route("/live-data", get(|| async { Json(json!({"data":{"totalUsers":2,"perWorld":[{"worldName":"test.eth","users":2}]}})) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let cfg = Config {
        http_host: "127.0.0.1".into(),
        http_port: 0,
        database_url: String::new(),
        archipelago_url: base.clone(),
        comms_url: base.clone(),
        worlds_server_url: base,
        genesis_realm: "main".into(),
        snapshot_interval_secs: 300,
    };
    let collector = Collector::new(db.pool.clone(), UpstreamClient::new(&cfg).unwrap());
    let summary = collector.snapshot().await.unwrap();
    assert_eq!(
        (
            summary.peers,
            summary.islands,
            summary.hot_scenes,
            summary.scene_users,
            summary.world_users
        ),
        (514, 1, 1, 2, 2)
    );
    let (count, nulls): (i64,i64) = sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE parcel_x IS NULL AND last_ping IS NULL) FROM peer_snapshots").fetch_one(&db.pool).await.unwrap();
    assert_eq!((count, nulls), (513, 513));
    let (addresses, count, live_users): (Value, i32, i32) =
        sqlx::query_as("SELECT addresses, count, live_users FROM world_membership")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        (addresses, count, live_users),
        (json!(["0xabc", "0xdef"]), 2, 2)
    );
    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM insert_calls")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(calls, 6, "one parent, five unnest child tables");
    sqlx::query("ALTER TABLE hot_scene_snapshots ADD CONSTRAINT fail_next CHECK (snapshot_id = 1)")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(collector.snapshot().await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM snapshots")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "failure must roll back the entire snapshot");
    server.abort();
    drop(collector);
    db.drop().await;
}
