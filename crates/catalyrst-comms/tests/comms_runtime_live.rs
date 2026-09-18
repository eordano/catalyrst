#![cfg(feature = "nats")]

#[path = "support/mod.rs"]
mod app_support;
#[path = "support/nats.rs"]
mod nats_support;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use catalyrst_comms::config::ClusterConfig;
use catalyrst_comms::CommsRuntime;
use catalyrst_pulse::cluster::nats::{NatsClusterFeed, NatsFeedOptions};
use catalyrst_pulse::cluster::{ClusterOptions, ClusterTracker};
use catalyrst_pulse::decentraland::common::Vector3;
use catalyrst_pulse::decentraland::pulse::PeerClusterSnapshot;
use catalyrst_pulse::interest::SPATIAL_GRID_CELL_SIZE;
use catalyrst_pulse::realm_grids::RealmSpatialGrids;
use catalyrst_pulse::snapshot::{IdentityBoard, PeerSnapshot, SnapshotBoard};
use futures::StreamExt;
use prost::Message;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool, Row};

const AUDIENCE: &str = "comms-runtime-live";
const WALLET: &str = "0x1111111111111111111111111111111111111111";
const SESSION: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WAIT: Duration = Duration::from_secs(12);

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    schema: String,
    scoped_url: String,
}

impl Fixture {
    async fn new() -> Option<Self> {
        let url = std::env::var("CATALYRST_COMMS_TEST_PG")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("CATALYRST_ARCHIPELAGO_TEST_PG")
                    .ok()
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| catalyrst_testgate::require_pg("CATALYRST_COMMS_TEST_PG"))?;
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect configured PostgreSQL");
        let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
            .fetch_one(&admin)
            .await
            .expect("read PostgreSQL version");
        assert!(version >= 180000, "this regression requires PostgreSQL 18");

        let schema = format!("comms_runtime_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create isolated schema");
        let options = PgConnectOptions::from_str(&url)
            .expect("parse PostgreSQL URL")
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .connect_with(options)
            .await
            .expect("connect isolated schema");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run comms migrations");
        sqlx::query(
            "CREATE TABLE archipelago_v4_assignments (
                deployment_audience text NOT NULL,
                owner_address text NOT NULL,
                lane_key text NOT NULL,
                authority_incarnation text NOT NULL,
                assignment_revision bigint NOT NULL,
                owner_session text NOT NULL,
                owner_epoch bigint NOT NULL,
                fencing_token bigint NOT NULL,
                assignment_json jsonb,
                acknowledged_revision bigint NOT NULL DEFAULT 0,
                updated_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY (deployment_audience, owner_address, lane_key)
            )",
        )
        .execute(&pool)
        .await
        .expect("create authority table");
        sqlx::query(
            "INSERT INTO archipelago_v4_assignments (
                deployment_audience, owner_address, lane_key, authority_incarnation,
                assignment_revision, owner_session, owner_epoch, fencing_token, assignment_json
             ) VALUES ($1, $2, 'realm', 'runtime-test-authority', 1, $3, 7, 11, NULL)",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .bind(SESSION)
        .execute(&pool)
        .await
        .expect("seed protected realm owner");
        let separator = if url.contains('?') { '&' } else { '?' };
        let scoped_url = format!("{url}{separator}options[search_path]={schema}");
        Some(Self {
            admin,
            pool,
            schema,
            scoped_url,
        })
    }

    async fn assignment(&self) -> (i64, Option<Value>) {
        let row = sqlx::query(
            "SELECT assignment_revision, assignment_json
             FROM archipelago_v4_assignments
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .fetch_one(&self.pool)
        .await
        .expect("read assignment row");
        (
            row.try_get("assignment_revision").unwrap(),
            row.try_get("assignment_json").unwrap(),
        )
    }

    async fn clear_assignment(&self) {
        sqlx::query(
            "UPDATE archipelago_v4_assignments
             SET assignment_json = NULL, updated_at = clock_timestamp()
             WHERE deployment_audience = $1 AND owner_address = $2 AND lane_key = 'realm'",
        )
        .bind(AUDIENCE)
        .bind(WALLET)
        .execute(&self.pool)
        .await
        .expect("clear assignment without changing ownership or revision");
    }

    async fn finish(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .expect("drop isolated schema");
        self.admin.close().await;
    }
}

struct World {
    grids: RealmSpatialGrids,
    board: SnapshotBoard,
    identity: IdentityBoard,
    tracker: ClusterTracker,
}

impl World {
    fn new(feed: Arc<NatsClusterFeed>) -> Self {
        let mut world = Self {
            grids: RealmSpatialGrids::new(SPATIAL_GRID_CELL_SIZE, 8),
            board: SnapshotBoard::new(8, 4),
            identity: IdentityBoard::new(8),
            tracker: ClusterTracker::new(ClusterOptions::default(), 8, feed),
        };
        for peer in 0..3 {
            let position = Vector3 {
                x: 1.0,
                y: 0.0,
                z: 1.0,
            };
            world.board.set_active(peer);
            world.board.publish(
                peer,
                PeerSnapshot {
                    global_position: position,
                    realm: Some("runtime-realm".into()),
                    ..Default::default()
                },
            );
            let (wallet, session) = if peer == 0 {
                (WALLET.to_string(), SESSION.to_string())
            } else {
                (format!("0x{peer:040x}"), format!("0x{:040x}", peer + 100))
            };
            world.identity.set_with_session(peer, wallet, session);
            world.grids.set(peer, "runtime-realm", position);
        }
        world
    }

    fn pass(&mut self) {
        self.tracker
            .run_pass(&self.grids, &self.board, &self.identity);
    }
}

async fn connect(url: &str) -> async_nats::Client {
    tokio::time::timeout(WAIT, async {
        loop {
            match async_nats::connect(url).await {
                Ok(client) => break client,
                Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
    })
    .await
    .expect("connect to test-owned NATS broker")
}

async fn await_runtime(runtime: &CommsRuntime) {
    tokio::time::timeout(WAIT, async {
        while !runtime.is_ready() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("embedded runtime connects to its broker");
}

async fn await_runtime_disconnect(runtime: &CommsRuntime) {
    tokio::time::timeout(WAIT, async {
        while runtime.is_ready() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("embedded runtime observes the broker outage");
}

async fn await_source(client: &async_nats::Client) -> PeerClusterSnapshot {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(reply) = client
                .request(
                    format!("peer.{WALLET}.cluster_lookup"),
                    SESSION.as_bytes().to_vec().into(),
                )
                .await
            {
                break PeerClusterSnapshot::decode(reply.payload).expect("decode live source");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Pulse feed exposes current cluster source")
}

async fn await_assignment(fixture: &Fixture) {
    tokio::time::timeout(WAIT, async {
        while fixture.assignment().await.1.is_none() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("embedded runtime commits a fenced assignment");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn embedded_runtime_reconciles_protected_assignments_and_survives_broker_restart() {
    let Some(binary) = catalyrst_testgate::require_env("COMMS_NATS_SERVER_BIN") else {
        return;
    };
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let mut broker = Some(nats_support::Broker::start(&binary, None));
    let port = broker.as_ref().unwrap().port;
    let nats_url = broker.as_ref().unwrap().url();

    let feed = Arc::new(NatsClusterFeed::spawn(NatsFeedOptions {
        url: nats_url.clone(),
        discovery_interval_ms: 100,
        ..Default::default()
    }));
    let mut world = World::new(feed);
    let mut config = ClusterConfig {
        nats_url: Some(nats_url.clone()),
        enabled: true,
        queue_group: format!("comms-runtime-{}", uuid::Uuid::new_v4().simple()),
        control_database_url: Some(fixture.scoped_url.clone()),
        control_v4_audience: Some(AUDIENCE.into()),
        takeover_retry_delay_ms: 0,
        ..Default::default()
    };
    config.drain_timeout_ms = 2_000;
    let state = app_support::test_state(fixture.pool.clone(), None, None, "unused");
    let mut runtime = CommsRuntime::embedded(state, &config).expect("build embedded runtime");
    runtime.start();
    runtime.start();

    let client = connect(&nats_url).await;
    let mut changes = client
        .subscribe(format!("peer.{WALLET}.cluster_change"))
        .await
        .expect("observe the production Pulse event");
    nats_support::round_trip(&client).await;
    await_runtime(&runtime).await;
    world.pass();
    let change = tokio::time::timeout(WAIT, changes.next())
        .await
        .expect("Pulse publishes the initial cluster change")
        .expect("cluster change subscription remains open")
        .payload;
    let source = await_source(&client).await;
    assert!(!source.cluster_id.is_empty());
    await_assignment(&fixture).await;
    let (revision, assignment) = fixture.assignment().await;
    assert_eq!(
        revision, 2,
        "idempotent start must not duplicate the commit"
    );
    let assignment = assignment.unwrap();
    assert!(assignment["islandId"]
        .as_str()
        .unwrap()
        .starts_with("island-"));
    assert!(assignment["connectionString"]
        .as_str()
        .unwrap()
        .starts_with("livekit:wss://livekit.local"));

    fixture.clear_assignment().await;
    drop(client);
    drop(broker.take());
    await_runtime_disconnect(&runtime).await;
    broker = Some(nats_support::Broker::start(&binary, Some(port)));
    assert_eq!(broker.as_ref().unwrap().url(), nats_url);
    let client = connect(&nats_url).await;
    nats_support::round_trip(&client).await;
    await_runtime(&runtime).await;
    world.pass();
    let source = await_source(&client).await;
    assert!(!source.cluster_id.is_empty());
    client
        .publish(format!("peer.{WALLET}.cluster_change"), change)
        .await
        .expect("publish cluster change after broker recovery");
    await_assignment(&fixture).await;
    let (revision, assignment) = fixture.assignment().await;
    assert_eq!(
        revision, 3,
        "broker recovery must commit exactly one new assignment"
    );
    assert!(assignment.is_some());

    runtime.shutdown().await;
    drop(client);
    drop(broker.take());
    fixture.finish().await;
}
