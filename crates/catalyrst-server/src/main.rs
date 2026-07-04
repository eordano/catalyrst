#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use serde_json::Value;
use tracing_subscriber::EnvFilter;

use catalyrst_server::routes::build_router;
use catalyrst_server::state::*;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
}

struct StubStorage;

#[async_trait]
impl ContentStorage for StubStorage {
    async fn retrieve(&self, _hash: &str) -> Option<Bytes> {
        None
    }
    async fn retrieve_stream(&self, _hash: &str) -> Option<(Body, u64)> {
        None
    }
    async fn retrieve_range(&self, _hash: &str, _start: u64, _end: u64) -> Option<Bytes> {
        None
    }
    async fn file_info(&self, _hash: &str) -> Option<FileInfo> {
        None
    }
    async fn exist_multiple(&self, hashes: &[String]) -> HashMap<String, bool> {
        hashes.iter().map(|h| (h.clone(), false)).collect()
    }
}

struct StubDatabase;

#[async_trait]
impl Database for StubDatabase {
    async fn active_entities_by_pointers(
        &self,
        _pointers: &[String],
    ) -> Result<Vec<Value>, DatabaseError> {
        Ok(vec![])
    }
    async fn active_entities_by_ids(&self, _ids: &[String]) -> Result<Vec<Value>, DatabaseError> {
        Ok(vec![])
    }
    async fn active_entities_by_prefix(
        &self,
        _prefix: &str,
        _offset: i64,
        _limit: i64,
    ) -> Result<PrefixQueryResult, DatabaseError> {
        Ok(PrefixQueryResult {
            total: 0,
            entities: vec![],
        })
    }
    async fn active_entity_ids_by_content_hash(
        &self,
        _hash: &str,
    ) -> Result<Vec<String>, DatabaseError> {
        Ok(vec![])
    }
    async fn get_deployments(
        &self,
        _options: &DeploymentQueryOptions,
    ) -> Result<DeploymentQueryResult, DatabaseError> {
        Ok(DeploymentQueryResult {
            deployments: vec![],
            filters: Value::Object(Default::default()),
            pagination: PaginationResult {
                offset: 0,
                limit: 100,
                more_data: false,
                next: None,
                last_id: None,
            },
        })
    }
    async fn get_pointer_changes(
        &self,
        _options: &PointerChangesQueryOptions,
    ) -> Result<PointerChangesQueryResult, DatabaseError> {
        Ok(PointerChangesQueryResult {
            deltas: vec![],
            filters: Value::Object(Default::default()),
            pagination: PaginationResult {
                offset: 0,
                limit: 100,
                more_data: false,
                next: None,
                last_id: None,
            },
        })
    }
    async fn get_failed_deployments(&self) -> Result<Vec<Value>, DatabaseError> {
        Ok(vec![])
    }
    async fn get_audit_info(
        &self,
        _entity_type: &str,
        _entity_id: &str,
    ) -> Result<Option<Value>, DatabaseError> {
        Ok(None)
    }
    async fn find_entity_by_pointer(&self, _pointer: &str) -> Result<Option<Value>, DatabaseError> {
        Ok(None)
    }
}

struct StubDeployer;

#[async_trait]
impl Deployer for StubDeployer {
    async fn deploy_entity(
        &self,
        _files: Vec<Bytes>,
        _entity_id: &str,
        _auth_chain: Value,
        _context: &str,
    ) -> Result<i64, Vec<String>> {
        Err(vec!["Stub deployer: not implemented".to_string()])
    }
}

struct StubDenylist;
impl Denylist for StubDenylist {
    fn is_denylisted(&self, _id: &str) -> bool {
        false
    }
}

struct StubChallengeSupervisor;
impl ChallengeSupervisor for StubChallengeSupervisor {
    fn get_challenge_text(&self) -> String {
        format!("dcl-challenge-{}", chrono::Utc::now().timestamp_millis())
    }
}

struct StubSynchronizationState;
impl SynchronizationState for StubSynchronizationState {
    fn get_state(&self) -> String {
        "Synced".to_string()
    }
}

struct StubSnapshotGenerator;
impl SnapshotGenerator for StubSnapshotGenerator {
    fn get_current_snapshots(&self) -> Option<Value> {
        None
    }
}

struct StubContentCluster;
#[async_trait]
impl ContentCluster for StubContentCluster {
    fn get_status(&self) -> Value {
        serde_json::json!({})
    }
}

struct AtomicAcceptingUsers(std::sync::atomic::AtomicBool);
impl AcceptingUsers for AtomicAcceptingUsers {
    fn is_accepting(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn set_accepting(&self, accepting: bool) -> Result<(), String> {
        self.0
            .store(accepting, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let port: u16 = env_or("HTTP_SERVER_PORT", "5140")
        .parse()
        .expect("HTTP_SERVER_PORT must be a valid port number");
    let host = env_or("HTTP_SERVER_HOST", "0.0.0.0");
    let content_version = env_or("CONTENT_VERSION", "7.6.1+rust");
    let lambdas_version = env_or("LAMBDAS_VERSION", "4.12.0+rust");
    let commit_hash = env_or("COMMIT_HASH", "unknown");
    let eth_network = env_or("ETH_NETWORK", "mainnet");
    let content_server_address = env_or(
        "CONTENT_SERVER_ADDRESS",
        &format!("http://{}:{}", host, port),
    );
    let read_only = env_bool("READ_ONLY", false);

    tracing::info!(
        content_version = %content_version,
        lambdas_version = %lambdas_version,
        commit = %commit_hash,
        eth_network = %eth_network,
        read_only = %read_only,
        "Starting catalyrst-server"
    );

    if read_only {
        tracing::info!(
            "Content Server running in read-only mode. POST /entities will not be exposed"
        );
    }

    let content_public_url = env_or("CONTENT_URL", &format!("http://{}:{}/content", host, port));
    let lambdas_public_url = env_or("LAMBDAS_URL", &format!("http://{}:{}/lambdas", host, port));
    let realm_name = std::env::var("REALM_NAME").ok();

    let profile_cdn_base_url = env_or(
        "PROFILE_CDN_BASE_URL",
        "https://profile-images.decentraland.org",
    );

    let land_image_base_url = env_or("LAND_IMAGE_BASE_URL", "https://api.decentraland.org");

    let state = Arc::new(AppState {
        storage: Arc::new(StubStorage),
        database: Arc::new(StubDatabase),
        deployer: Arc::new(StubDeployer),
        denylist: Arc::new(StubDenylist),
        challenge_supervisor: Arc::new(StubChallengeSupervisor),
        synchronization_state: Arc::new(StubSynchronizationState),
        snapshot_generator: Arc::new(StubSnapshotGenerator),
        content_cluster: Arc::new(StubContentCluster),
        accepting_users: Arc::new(AtomicAcceptingUsers(std::sync::atomic::AtomicBool::new(
            true,
        ))),
        deployments_cache: dashmap::DashMap::new(),
        content_version,
        lambdas_version,
        commit_hash,
        eth_network,
        content_server_address,
        read_only: std::sync::atomic::AtomicBool::new(read_only),
        audit_pool: None,
        entities_cache_control_max_age: std::env::var("ENTITIES_CACHE_CONTROL_MAX_AGE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10),
        content_public_url,
        lambdas_public_url,
        realm_name,
        squid_pool: None,
        profile_cdn_base_url,
        land_image_base_url,
    });

    let app = build_router(state);

    let bind_addr = format!("{}:{}", host, port);
    tracing::info!(addr = %bind_addr, "Listening");

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind TCP listener");

    axum::serve(listener, app).await.expect("Server error");
}
