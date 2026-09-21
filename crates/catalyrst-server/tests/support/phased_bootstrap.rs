use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use catalyrst_server::sync::batch_deployer::{BatchDeployer, BatchDeployerConfig};
use catalyrst_server::sync::sync_orchestrator::SyncOrchestratorConfig;
use catalyrst_server::sync::{
    LiveDeploymentRepository, LiveFailedDeploymentsStore, LiveProcessedSnapshotStore,
    LiveSyncDeployer, SyncOrchestrator, SyncState,
};
use serde_json::json;
use tokio::sync::Semaphore;

#[tokio::test]
async fn phased_catchup_does_not_skip_profiles_older_than_its_replay_window() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("catalyrst_server::sync=debug")
        .with_test_writer()
        .try_init();
    let Some((pool, schema)) = super::setup_db().await else {
        return;
    };
    let migration = include_str!("../../migrations/0001_content_schema.sql").replace("public.", "");
    let mut statement = String::new();
    for line in migration
        .lines()
        .filter(|line| !line.trim().starts_with("--"))
    {
        statement.push_str(line);
        statement.push('\n');
        if line.trim().ends_with(';') {
            sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&pool)
                .await
                .unwrap();
            statement.clear();
        }
    }
    sqlx::query("ALTER TABLE active_pointers ADD COLUMN IF NOT EXISTS entity_type text")
        .execute(&pool)
        .await
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    let floor = now - 3_600_000;
    let mut files = HashMap::new();
    let mut deltas = Vec::new();
    for (kind, timestamp) in [("profile", now - 1_800_000), ("scene", now)] {
        let bytes = serde_json::to_vec(&json!({
            "version": "v3", "type": kind, "timestamp": timestamp,
            "pointers": [format!("{kind}-pointer")], "content": [], "metadata": {}
        }))
        .unwrap();
        let id = catalyrst_hashing::hash_bytes_v1(&bytes);
        files.insert(id.clone(), bytes);
        deltas.push(json!({
            "entityId": id, "entityType": kind, "entityTimestamp": timestamp,
            "localTimestamp": timestamp, "authChain": [], "pointers": [format!("{kind}-pointer")]
        }));
    }
    let snapshot_calls = Arc::new(AtomicUsize::new(0));
    let profiles_entered = Arc::new(Semaphore::new(0));
    let profiles_resume = Arc::new(Semaphore::new(0));
    let app = Router::new()
        .route("/snapshots", get({
            let calls = snapshot_calls.clone();
            let entered = profiles_entered.clone();
            let resume = profiles_resume.clone();
            move || {
                let (calls, entered, resume) = (calls.clone(), entered.clone(), resume.clone());
                async move {
                    if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                        entered.add_permits(1);
                        resume.acquire().await.unwrap().forget();
                    }
                    Json(json!([]))
                }
            }
        }))
        .route("/pointer-changes", get(move |Query(query): Query<HashMap<String, String>>| {
            let deltas = deltas.clone();
            async move {
                let from = query["from"].parse::<i64>().unwrap();
                Json(json!({"deltas": deltas.into_iter().filter(|d| d["localTimestamp"].as_i64().unwrap() >= from).collect::<Vec<_>>(), "pagination": {"moreData": false}}))
            }
        }))
        .route("/contents/{id}", get(|State(files): State<Arc<HashMap<String, Vec<u8>>>>, Path(id): Path<String>| async move {
            files[&id].clone()
        }))
        .with_state(Arc::new(files));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let temp = std::env::temp_dir().join(format!("phased-bootstrap-{}", uuid::Uuid::new_v4()));
    let storage = Arc::new(
        catalyrst_storage::ContentStorage::new(temp.join("content"))
            .await
            .unwrap(),
    );
    let snapshots = Arc::new(
        catalyrst_storage::SnapshotStorage::new(temp.join("snapshots"))
            .await
            .unwrap(),
    );
    let repo = Arc::new(LiveDeploymentRepository::new(pool.clone()));
    repo.advance_server_sync_cursor(&server, floor)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let deployer = Arc::new(BatchDeployer::new(
        BatchDeployerConfig::default(),
        client.clone(),
        storage.clone(),
        Arc::new(LiveSyncDeployer::new(pool.clone())),
        repo.clone(),
        Arc::new(LiveFailedDeploymentsStore::new(pool.clone())),
    ));
    let orchestrator = SyncOrchestrator::new(
        SyncOrchestratorConfig {
            from_timestamp: floor,
            ..Default::default()
        },
        client,
        storage,
        deployer,
        Arc::new(LiveProcessedSnapshotStore::new(pool.clone())),
        snapshots,
        repo.clone(),
    );
    let handle = orchestrator
        .sync_with_servers([server.clone()].into_iter().collect())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), profiles_entered.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
    let partial_cursor = repo.get_server_sync_cursor(&server).await.unwrap();
    profiles_resume.add_permits(1);
    tokio::time::timeout(Duration::from_secs(15), async {
        while orchestrator.state().await != SyncState::Syncing {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    orchestrator.stop().await;
    drop(handle);
    http.abort();
    let types: Vec<String> =
        sqlx::query_scalar("SELECT entity_type FROM deployments ORDER BY entity_type")
            .fetch_all(&pool)
            .await
            .unwrap();
    let final_cursor = repo.get_server_sync_cursor(&server).await.unwrap();
    let failures: Vec<String> =
        sqlx::query_scalar("SELECT error_description FROM failed_deployments")
            .fetch_all(&pool)
            .await
            .unwrap();
    super::teardown(&pool, &schema).await;
    std::fs::remove_dir_all(temp).unwrap();
    assert!(
        failures.is_empty(),
        "fixture deployments failed: {failures:?}"
    );
    assert_eq!(
        types,
        ["profile", "scene"],
        "a profile outside the 20-minute replay window must survive the phase handoff"
    );
    assert_eq!(
        partial_cursor,
        Some(floor),
        "a partial snapshot pass cannot publish a shared cursor"
    );
    assert_eq!(final_cursor, Some(now));
}
