#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context};
use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use tokio::sync::{Mutex, RwLock};
use tokio_util::io::ReaderStream;
use tracing_subscriber::EnvFilter;

use catalyrst_envcfg::env_bool;
use catalyrst_server::routes::build_router;
use catalyrst_server::state::*;

mod cache;
mod db;
mod rows;
mod services;
mod storage;

use cache::*;
use db::*;
use rows::*;
use services::*;
use storage::*;

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5141)"),
    (
        "PUBLIC_URL",
        "public base URL (default http://HOST:PORT)",
    ),
    (
        "CONTENT_SERVER_ADDRESS",
        "advertised content server address (default PUBLIC_URL/content)",
    ),
    (
        "CONTENT_URL",
        "public content URL (default PUBLIC_URL/content/)",
    ),
    (
        "LAMBDAS_URL",
        "public lambdas URL (default PUBLIC_URL/lambdas/)",
    ),
    (
        "CONTENT_VERSION",
        "reported content server version (default 7.6.1+rust)",
    ),
    (
        "LAMBDAS_VERSION",
        "reported lambdas version (default 4.12.0+rust)",
    ),
    ("COMMIT_HASH", "reported commit hash (default unknown)"),
    ("ETH_NETWORK", "ethereum network (default mainnet)"),
    ("REALM_NAME", "optional — realm name"),
    (
        "MAP_SATELLITE_BASE_URL",
        "minimap satellite tiles base URL (default https://genesis.city/map/latest)",
    ),
    (
        "MAP_SATELLITE_SUFFIX",
        "minimap satellite tile suffix (default .jpg)",
    ),
    (
        "MAP_PARCEL_VIEW_URL",
        "minimap parcel view image URL (default https://api.decentraland.org/v1/minimap.png)",
    ),
    (
        "POSTGRES_HOST",
        "postgres host or unix socket dir (default /run/postgresql)",
    ),
    ("POSTGRES_PORT", "postgres port (default 5432)"),
    (
        "POSTGRES_CONTENT_USER",
        "required — content DB user (env var or /etc/catalyrst/content.env)",
    ),
    (
        "POSTGRES_CONTENT_PASSWORD",
        "required — content DB password (env var or /etc/catalyrst/content.env)",
    ),
    ("POSTGRES_CONTENT_DB", "content DB name (default content)"),
    ("PG_POOL_SIZE", "main postgres pool max connections (default 50)"),
    (
        "SQUID_PG_POOL_SIZE",
        "squid postgres pool max connections (default 10)",
    ),
    (
        "SYNC_PG_POOL_SIZE",
        "sync postgres pool max connections (default 40)",
    ),
    (
        "STORAGE_ROOT_FOLDER",
        "content blob root (default /var/lib/catalyrst/content)",
    ),
    (
        "SYNC_ENABLED",
        "bool — enable the sync orchestrator (default false)",
    ),
    (
        "SYNC_SOURCE",
        "comma-separated sync peer URLs (default http://127.0.0.1:5140)",
    ),
    (
        "SYNC_STORAGE_ROOT",
        "sync blob root (default /var/lib/catalyrst/content_rust)",
    ),
    ("SYNC_DB_NAME", "sync DB name (default content_rust)"),
    (
        "CONCURRENT_SYNC_DOWNLOADS",
        "sync content download concurrency (default 200)",
    ),
    (
        "CONNECTIONS_MAX_IDLE",
        "sync HTTP client max idle connections per host (default 25)",
    ),
    ("PHASED_SYNC", "bool — phased sync (default true)"),
    (
        "RETRY_FAILED_ENABLED",
        "bool — retry-failed-deployments worker (default true)",
    ),
    (
        "RETRY_FAILED_PRUNE_TTL_DAYS",
        "failed_deployments prune TTL in days (default 7)",
    ),
    (
        "SNAPSHOT_GENERATION_INTERVAL_HOURS",
        "snapshot generation interval in hours (default 6)",
    ),
    ("SQUID_DB_HOST", "squid DB host (default POSTGRES_HOST)"),
    (
        "SQUID_DB_PORT",
        "squid DB port (default POSTGRES_PORT; parse fallback 6432)",
    ),
    ("SQUID_DB_USER", "squid DB user (default squid_ro)"),
    ("SQUID_DB_PASSWORD", "optional — squid DB password"),
    (
        "SQUID_DB_NAME",
        "squid DB name (default marketplace_squid)",
    ),
    (
        "THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL",
        "TPR subgraph URL (mainnet default https://subgraph.decentraland.org/tpr-matic-mainnet)",
    ),
    (
        "BLOCKS_L2_SUBGRAPH_URL",
        "blocks subgraph URL (mainnet default https://subgraph.decentraland.org/blocks-matic-mainnet)",
    ),
    (
        "THIRD_PARTY_REFRESH_HOURS",
        "third-party root refresh interval in hours (unset/0 disables)",
    ),
    (
        "ENABLE_DEPLOYMENTS",
        "\"true\" serves authoritative writes on POST /entities (default false)",
    ),
    (
        "IGNORE_BLOCKCHAIN_ACCESS_CHECKS",
        "\"true\" skips blockchain access checks on writes (default false)",
    ),
    (
        "ETH_RPC_URL",
        "https RPC endpoint for write validation (default https://rpc.decentraland.org/mainnet)",
    ),
    (
        "ADDITIONAL_DECENTRALAND_ADDRESS",
        "optional — extra address accepted as decentraland for write validation",
    ),
    (
        "THIRD_PARTY_ROOT_SOURCE",
        "\"subgraph\" or \"squid\" (default subgraph)",
    ),
    (
        "READ_ONLY",
        "bool — start with POST /entities disabled (default false)",
    ),
    (
        "ENTITIES_CACHE_CONTROL_MAX_AGE",
        "entities Cache-Control max-age in seconds (default 10)",
    ),
    ("RUST_LOG", "tracing filter (default info)"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-live", ENV_DOCS);

    load_env_file("/etc/catalyrst/content.env");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    catalyrst_server::metrics::init();

    let port: u16 = env_or("HTTP_SERVER_PORT", "5141")
        .parse()
        .context("HTTP_SERVER_PORT must be a valid port number")?;
    let host = env_or("HTTP_SERVER_HOST", "127.0.0.1");
    let content_version = env_or("CONTENT_VERSION", "7.6.1+rust");
    let lambdas_version = env_or("LAMBDAS_VERSION", "4.12.0+rust");
    let commit_hash = env_or("COMMIT_HASH", "unknown");
    let eth_network = env_or("ETH_NETWORK", "mainnet");
    let public_url = env_or("PUBLIC_URL", &format!("http://{}:{}", host, port))
        .trim_end_matches('/')
        .to_string();
    let content_server_address =
        env_or("CONTENT_SERVER_ADDRESS", &format!("{}/content", public_url));

    let pg_host = env_or("POSTGRES_HOST", "/run/postgresql");
    let pg_port = env_or("POSTGRES_PORT", "5432");
    let pg_user = env_or("POSTGRES_CONTENT_USER", "");
    if pg_user.is_empty() {
        bail!("POSTGRES_CONTENT_USER must be set (env var or /etc/catalyrst/content.env)");
    }
    let pg_password = env_or("POSTGRES_CONTENT_PASSWORD", "");
    if pg_password.is_empty() {
        bail!("POSTGRES_CONTENT_PASSWORD must be set (env var or /etc/catalyrst/content.env)");
    }
    let pg_db = env_or("POSTGRES_CONTENT_DB", "content");

    let db_url = if pg_host.starts_with('/') {
        format!(
            "postgres://{}:{}@localhost:{}/{}?host={}",
            pg_user, pg_password, pg_port, pg_db, pg_host
        )
    } else {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            pg_user, pg_password, pg_host, pg_port, pg_db
        )
    };

    tracing::info!(
        db = %pg_db,
        host = %pg_host,
        port = %pg_port,
        "Connecting to postgres"
    );

    let connect_options: PgConnectOptions = db_url
        .parse::<PgConnectOptions>()
        .context(
            "failed to parse the postgres URL assembled from POSTGRES_HOST/POSTGRES_PORT/\
             POSTGRES_CONTENT_USER/POSTGRES_CONTENT_PASSWORD/POSTGRES_CONTENT_DB",
        )?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);

    let pg_pool_size: u32 = env_or("PG_POOL_SIZE", "50")
        .parse()
        .context("PG_POOL_SIZE must be a number")?;
    let pool = PgPoolOptions::new()
        .max_connections(pg_pool_size)
        .min_connections(10)
        .idle_timeout(std::time::Duration::from_secs(600))
        .max_lifetime(std::time::Duration::from_secs(3600))
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(connect_options)
        .await
        .with_context(|| {
            format!(
                "failed to connect to postgres db {:?} at {}:{} \
                 (POSTGRES_HOST/POSTGRES_PORT/POSTGRES_CONTENT_USER/\
                 POSTGRES_CONTENT_PASSWORD/POSTGRES_CONTENT_DB)",
                pg_db, pg_host, pg_port
            )
        })?;

    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .context("database connectivity check (SELECT 1) failed")?;
    tracing::info!("Database connection verified");

    tracing::info!("Pre-warming prepared statement cache");
    let _ = sqlx::query("SELECT 1 FROM deployments WHERE entity_type = $1 LIMIT 0")
        .bind("profile")
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM content_files WHERE deployment = ANY($1::int[]) LIMIT 0")
        .bind(&[0i32][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM active_pointers WHERE pointer = ANY($1::text[]) LIMIT 0")
        .bind(&[""][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "SELECT 1 FROM deployments WHERE entity_id = ANY($1::text[]) AND deleter_deployment IS NULL LIMIT 0"
    )
        .bind(&[""][..])
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM deployments WHERE entity_id = $1 LIMIT 0")
        .bind("")
        .execute(&pool)
        .await;
    let _ = sqlx::query("SELECT 1 FROM failed_deployments LIMIT 0")
        .execute(&pool)
        .await;
    tracing::info!("Prepared statement cache warmed");

    let storage_root = env_or("STORAGE_ROOT_FOLDER", "/var/lib/catalyrst/content");
    tracing::info!(root = %storage_root, "Initializing content storage");

    let content_storage = catalyrst_storage::ContentStorage::new(&storage_root)
        .await
        .expect("Failed to initialize content storage");

    let entity_cache = Arc::new(RwLock::new(EntityCache::new()));
    let profile_lru = Arc::new(Mutex::new(ProfileLru::new(10_000)));

    let prefix_ids_cache = Arc::new(Mutex::new(PrefixIdsCache::new(
        2_000,
        std::time::Duration::from_secs(24 * 60 * 60),
    )));

    let sync_enabled = env_bool("SYNC_ENABLED", false);

    if !sync_enabled {
        tracing::info!("Loading non-profile entities into memory cache...");
        for entity_type in &["scene", "wearable", "emote", "store", "outfits"] {
            let mut ec = entity_cache.write().await;
            if let Err(e) = load_entity_type_into_cache(&pool, &mut ec, entity_type).await {
                tracing::warn!(entity_type = %entity_type, error = %e, "Failed to load entity type into cache");
            }
        }
        {
            let ec = entity_cache.read().await;
            tracing::info!(
                total = ec.by_id.len(),
                pointers = ec.pointer_to_id.len(),
                "Entity cache loaded"
            );
        }

        let _ = install_notify_trigger(&pool).await;
        tokio::spawn(listen_for_invalidations(
            pool.clone(),
            entity_cache.clone(),
            profile_lru.clone(),
            prefix_ids_cache.clone(),
        ));
    } else {
        tracing::info!("Sync mode — skipping entity cache load and NOTIFY listener");
    }

    let content_public_url = env_or("CONTENT_URL", &format!("{}/content/", public_url));
    let lambdas_public_url = env_or("LAMBDAS_URL", &format!("{}/lambdas/", public_url));
    let realm_name = std::env::var("REALM_NAME").ok();

    let profile_cdn_base_url = env_or(
        "PROFILE_CDN_BASE_URL",
        "https://profile-images.decentraland.org",
    );

    let land_image_base_url = env_or("LAND_IMAGE_BASE_URL", "https://api.decentraland.org");
    if ["127.0.0.1", "localhost", "[::1]", "0.0.0.0"]
        .iter()
        .any(|lo| land_image_base_url.contains(lo))
    {
        tracing::warn!(
            base = %land_image_base_url,
            "LAND image URLs use a LOOPBACK base — clients cannot fetch them; \
             set LAND_IMAGE_BASE_URL to the public gateway base"
        );
    }

    let squid_pool = {
        let squid_host = env_or("SQUID_DB_HOST", &pg_host);
        let squid_port: u16 = env_or("SQUID_DB_PORT", &pg_port).parse().unwrap_or(6432);
        let squid_user = env_or("SQUID_DB_USER", "squid_ro");
        let squid_password = std::env::var("SQUID_DB_PASSWORD").ok();
        let squid_db = env_or("SQUID_DB_NAME", "marketplace_squid");

        let squid_opts = sqlx::postgres::PgConnectOptions::new()
            .host(&squid_host)
            .port(squid_port)
            .username(&squid_user)
            .database(&squid_db)
            .options([
                ("statement_timeout", "60000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ]);

        let squid_opts = match squid_password {
            Some(ref pw) => squid_opts.password(pw),
            None => squid_opts,
        };

        let squid_pool_size: u32 = env_or("SQUID_PG_POOL_SIZE", "10").parse().unwrap_or(10);
        match sqlx::postgres::PgPoolOptions::new()
            .max_connections(squid_pool_size)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect_with(squid_opts)
            .await
        {
            Ok(p) => {
                tracing::info!(db = %squid_db, "Connected to squid database for ownership validation");
                Some(p)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Could not connect to squid database — ownership validation disabled (all items pass through)"
                );
                None
            }
        }
    };

    let sync_gauges = catalyrst_server::sync_backends::SyncGauges::default();
    let sync_orchestrator = if sync_enabled {
        let sync_source = env_or("SYNC_SOURCE", "http://127.0.0.1:5140");
        tracing::info!(source = %sync_source, "Preparing sync orchestrator");

        for raw in sync_source.split(',') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            match reqwest::Url::parse(trimmed) {
                Ok(u) => {
                    if u.scheme() == "http" {
                        let host = u.host_str().unwrap_or("");
                        let is_loopback = host == "localhost"
                            || host == "127.0.0.1"
                            || host == "[::1]"
                            || host == "::1";
                        if !is_loopback {
                            panic!(
                                "SYNC_SOURCE entry '{trimmed}' uses plaintext http:// to a \
                                 non-loopback host ({host}); sync ingests arbitrary entities \
                                 from this peer, so a MITM could poison the index. Use \
                                 https:// or set the peer to 127.0.0.1/localhost for dev."
                            );
                        }
                    }
                }
                Err(e) => {
                    panic!("SYNC_SOURCE entry '{trimmed}' is not a valid URL: {e}");
                }
            }
        }

        let sync_storage_root = env_or("SYNC_STORAGE_ROOT", "/var/lib/catalyrst/content_rust");
        let sync_storage =
            std::sync::Arc::new(catalyrst_server::sync_backends::LiveSyncStorage::new(
                catalyrst_storage::ContentStorage::new(&sync_storage_root)
                    .await
                    .expect("Failed to create sync content storage"),
            ));

        let sync_db_name = env_or("SYNC_DB_NAME", "content_rust");
        let sync_pg_user = env_or("POSTGRES_CONTENT_USER", "");
        if sync_pg_user.is_empty() {
            panic!("POSTGRES_CONTENT_USER must be set for sync pool (env var or /etc/catalyrst/content.env)");
        }
        let sync_pg_password = env_or("POSTGRES_CONTENT_PASSWORD", "");
        if sync_pg_password.is_empty() {
            panic!("POSTGRES_CONTENT_PASSWORD must be set for sync pool (env var or /etc/catalyrst/content.env)");
        }
        let sync_opts = sqlx::postgres::PgConnectOptions::new()
            .host(&env_or("POSTGRES_HOST", "/run/postgresql"))
            .port(env_or("POSTGRES_PORT", "5432").parse().unwrap_or(5432))
            .username(&sync_pg_user)
            .password(&sync_pg_password)
            .database(&sync_db_name)
            .options([
                ("statement_timeout", "60000"),
                ("idle_in_transaction_session_timeout", "30000"),
            ]);
        tracing::info!(db = %sync_db_name, "Connecting to sync database");
        let sync_pool_size: u32 = env_or("SYNC_PG_POOL_SIZE", "40")
            .parse()
            .expect("SYNC_PG_POOL_SIZE must be a number");
        let sync_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(sync_pool_size)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect_with(sync_opts)
            .await
            .expect("Failed to connect to sync database");
        tracing::info!("Sync database connected");

        let sync_deployer: std::sync::Arc<dyn catalyrst_sync::Deployer> = std::sync::Arc::new(
            catalyrst_server::sync_backends::LiveSyncDeployer::new(sync_pool.clone()),
        );
        let sync_deploy_repo: std::sync::Arc<dyn catalyrst_sync::DeploymentRepository> =
            std::sync::Arc::new(
                catalyrst_server::sync_backends::LiveDeploymentRepository::with_gauges(
                    sync_pool.clone(),
                    sync_gauges.clone(),
                ),
            );
        let sync_failed: std::sync::Arc<dyn catalyrst_sync::FailedDeploymentsStore> =
            std::sync::Arc::new(
                catalyrst_server::sync_backends::LiveFailedDeploymentsStore::new(sync_pool.clone()),
            );
        let sync_processed: std::sync::Arc<dyn catalyrst_sync::ProcessedSnapshotStore> =
            std::sync::Arc::new(
                catalyrst_server::sync_backends::LiveProcessedSnapshotStore::new(sync_pool.clone()),
            );

        let snapshot_storage_path = format!("{}/snapshots", sync_storage_root);
        tokio::fs::create_dir_all(&snapshot_storage_path).await.ok();
        let sync_snapshot_check: std::sync::Arc<dyn catalyrst_sync::SnapshotStorageCheck> =
            std::sync::Arc::new(
                catalyrst_server::sync_backends::LiveSnapshotStorageCheck::new(
                    catalyrst_storage::SnapshotStorage::new(&snapshot_storage_path)
                        .await
                        .expect("Failed to create snapshot storage"),
                ),
            );

        let content_download_concurrency: usize = env_or("CONCURRENT_SYNC_DOWNLOADS", "200")
            .parse()
            .expect("CONCURRENT_SYNC_DOWNLOADS must be a number");

        let connections_max_idle: usize = env_or("CONNECTIONS_MAX_IDLE", "25")
            .parse()
            .expect("CONNECTIONS_MAX_IDLE must be a number");

        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(connections_max_idle)
            .tcp_nodelay(true)
            .connect_timeout(std::time::Duration::from_secs(8))
            .read_timeout(std::time::Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::limited(2))
            .build()
            .expect("Failed to create HTTP client");

        let sync_deploy_repo_live =
            catalyrst_server::sync_backends::LiveDeploymentRepository::new(sync_pool.clone());
        let mut bloom = catalyrst_sync::BloomFilter::new();
        tracing::info!("Loading entity IDs into bloom filter...");
        match sync_deploy_repo_live.load_all_entity_ids().await {
            Ok(ids) => {
                let count = ids.len();
                for id in &ids {
                    bloom.add(id);
                }
                tracing::info!(count, "Bloom filter populated");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load entity IDs for bloom filter, starting empty");
            }
        }

        let batch_deployer =
            std::sync::Arc::new(catalyrst_sync::batch_deployer::BatchDeployer::with_bloom(
                catalyrst_sync::batch_deployer::BatchDeployerConfig {
                    content_download_concurrency,
                    ..Default::default()
                },
                http_client.clone(),
                sync_storage.clone(),
                sync_deployer.clone(),
                sync_deploy_repo.clone(),
                sync_failed.clone(),
                bloom,
            ));

        let retry_peers: Vec<String> = sync_source
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let retry_worker = catalyrst_sync::retry_failed::RetryFailedDeployments::new(
            catalyrst_sync::retry_failed::RetryFailedConfig::default(),
            http_client.clone(),
            sync_storage.clone(),
            sync_deployer.clone(),
            sync_failed.clone(),
            std::sync::Arc::new(tokio::sync::RwLock::new(retry_peers)),
        );
        let retry_pool = sync_pool.clone();

        let phased_sync = env_bool("PHASED_SYNC", true);

        let orchestrator = catalyrst_sync::sync_orchestrator::SyncOrchestrator::new(
            catalyrst_sync::sync_orchestrator::SyncOrchestratorConfig {
                from_timestamp: 0,
                request_max_retries: 10,
                request_retry_wait_ms: 5000,
                delete_snapshots_after_use: false,
                pointer_changes_wait_time_ms: 10_000,
                bootstrap_reconnect_time_ms: 5000,
                bootstrap_reconnect_exponent: 1.5,
                bootstrap_max_reconnect_ms: 3_600_000,
                syncing_reconnect_time_ms: 1000,
                syncing_reconnect_exponent: 1.5,
                syncing_max_reconnect_ms: 86_400_000,
                re_snapshot_interval_ms: 86_400_000 * 14,
                phased_sync,
            },
            http_client,
            sync_storage,
            batch_deployer,
            sync_processed,
            sync_snapshot_check,
            sync_deploy_repo,
        );

        tracing::info!(phased_sync, "Sync orchestrator ready");
        Some((orchestrator, sync_source, retry_worker, retry_pool))
    } else {
        None
    };

    let sync_state: Arc<dyn SynchronizationState> = match &sync_orchestrator {
        Some((orch, _, _, _)) => Arc::new(LiveSynchronizationState::with_sync_state(
            orch.state_handle(),
            Some(orch.control_handle()),
            sync_gauges.clone(),
        )),
        None => Arc::new(LiveSynchronizationState::new()),
    };

    let snapshot_gen = LiveSnapshotGenerator::load(&pool).await;
    let snapshot_handle = snapshot_gen.snapshots_handle();

    let snapshot_generation_interval_hours: u64 = env_or("SNAPSHOT_GENERATION_INTERVAL_HOURS", "6")
        .parse()
        .expect("SNAPSHOT_GENERATION_INTERVAL_HOURS must be a number");

    let snapshot_storage_path = format!("{}/snapshots", storage_root);
    tokio::fs::create_dir_all(&snapshot_storage_path).await.ok();

    let is_mainnet = eth_network == "mainnet";
    let tpr_subgraph_url = std::env::var("THIRD_PARTY_REGISTRY_L2_SUBGRAPH_URL")
        .ok()
        .or_else(|| {
            is_mainnet.then(|| "https://subgraph.decentraland.org/tpr-matic-mainnet".to_string())
        });
    let blocks_l2_subgraph_url = std::env::var("BLOCKS_L2_SUBGRAPH_URL").ok().or_else(|| {
        is_mainnet.then(|| "https://subgraph.decentraland.org/blocks-matic-mainnet".to_string())
    });

    if let (Some(hours), Some(sp), Some(tpr)) = (
        std::env::var("THIRD_PARTY_REFRESH_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|h| *h > 0),
        squid_pool.clone(),
        tpr_subgraph_url.clone(),
    ) {
        let tp = catalyrst_validator::tp_subgraph::TpSubgraph::new(
            blocks_l2_subgraph_url.clone().unwrap_or_default(),
            tpr,
        );
        tracing::info!(hours, "third-party root refresh enabled");
        catalyrst_server::third_party_refresh::spawn(
            sp,
            tp,
            std::time::Duration::from_secs(hours * 3600),
        );
    }

    let enable_deployments = env_or("ENABLE_DEPLOYMENTS", "false") == "true";
    let deployer: Arc<dyn Deployer> = if enable_deployments {
        let ignore_blockchain_access = env_or("IGNORE_BLOCKCHAIN_ACCESS_CHECKS", "false") == "true";
        let eth_rpc_url = env_or("ETH_RPC_URL", "https://rpc.decentraland.org/mainnet");
        if eth_rpc_url.starts_with("http://") {
            panic!(
                "ENABLE_DEPLOYMENTS=true but ETH_RPC_URL is plaintext http:// \
                 ({eth_rpc_url}); EIP-1654 signature validation requires a \
                 trusted TLS (https://) endpoint. Refusing to start."
            );
        }
        let additional_dcl_address = std::env::var("ADDITIONAL_DECENTRALAND_ADDRESS").ok();
        let third_party_root_via_squid = env_or("THIRD_PARTY_ROOT_SOURCE", "subgraph") == "squid";
        match squid_pool.clone() {
            Some(sp) => {
                let write_storage = catalyrst_storage::ContentStorage::new(&storage_root)
                    .await
                    .expect("failed to init content storage for write deployer");
                tracing::warn!(
                    ignore_blockchain_access,
                    "ENABLE_DEPLOYMENTS=true — serving authoritative writes on POST /entities"
                );
                Arc::new(catalyrst_server::write_deployer::WriteDeployer::new(
                    pool.clone(),
                    Arc::new(write_storage),
                    sp,
                    eth_rpc_url,
                    ignore_blockchain_access,
                    additional_dcl_address,
                    tpr_subgraph_url,
                    blocks_l2_subgraph_url,
                    third_party_root_via_squid,
                )) as Arc<dyn Deployer>
            }
            None => {
                tracing::error!(
                    "ENABLE_DEPLOYMENTS=true but no squid pool is available; \
                     refusing to enable writes (staying read-only)"
                );
                Arc::new(ReadOnlyDeployer) as Arc<dyn Deployer>
            }
        }
    } else {
        Arc::new(ReadOnlyDeployer) as Arc<dyn Deployer>
    };

    let state = Arc::new(AppState {
        storage: Arc::new(LiveContentStorage {
            inner: content_storage,
        }),
        database: Arc::new(LiveDatabase {
            pool: pool.clone(),
            entity_cache: entity_cache.clone(),
            profile_lru: profile_lru.clone(),
            prefix_ids_cache: prefix_ids_cache.clone(),
        }),
        deployer,
        denylist: Arc::new(MemoryDenylist::new()),
        challenge_supervisor: Arc::new(UuidChallengeSupervisor),
        synchronization_state: sync_state.clone(),
        snapshot_generator: Arc::new(snapshot_gen),
        content_cluster: Arc::new(LiveContentCluster),
        accepting_users: Arc::new(LiveAcceptingUsers(std::sync::atomic::AtomicBool::new(true))),
        deployments_cache: dashmap::DashMap::new(),
        content_version,
        lambdas_version,
        commit_hash,
        eth_network,
        content_server_address,
        read_only: std::sync::atomic::AtomicBool::new(env_bool("READ_ONLY", false)),

        audit_pool: Some(pool.clone()),
        entities_cache_control_max_age: env_or("ENTITIES_CACHE_CONTROL_MAX_AGE", "10")
            .parse()
            .unwrap_or(10),
        content_public_url,
        lambdas_public_url,
        realm_name,
        squid_pool,
        profile_cdn_base_url,
        land_image_base_url,
    });

    let app = build_router(state);

    if let Some((orchestrator, sync_source, retry_worker, retry_pool)) = sync_orchestrator {
        let peers: std::collections::HashSet<String> = sync_source
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        tokio::spawn(async move {
            tracing::info!(peers = ?peers, "Sync orchestrator starting...");
            match orchestrator.sync_with_servers(peers).await {
                Ok(handle) => {
                    tracing::info!("Sync started, waiting for bootstrap...");
                    handle.wait_for_bootstrap().await;
                    tracing::info!("Bootstrap complete, sync is now in steady state");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Sync orchestrator failed to start");
                }
            }
        });

        if env_bool("RETRY_FAILED_ENABLED", true) {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                retry_worker.run().await;
            });
        }

        let prune_ttl_days: i64 = env_or("RETRY_FAILED_PRUNE_TTL_DAYS", "7")
            .parse()
            .unwrap_or(7);
        tokio::spawn(async move {
            let day = std::time::Duration::from_secs(86400);
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            loop {
                match sqlx::query("DELETE FROM failed_deployments WHERE failure_time < NOW() - ($1 || ' days')::interval")
                    .bind(prune_ttl_days.to_string())
                    .execute(&retry_pool)
                    .await
                {
                    Ok(r) if r.rows_affected() > 0 => {
                        tracing::info!(pruned = r.rows_affected(), ttl_days = prune_ttl_days, "Pruned old failed_deployments");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to prune old failed_deployments");
                    }
                }
                tokio::time::sleep(day).await;
            }
        });
    }

    {
        let pool = pool.clone();
        let sync_state = sync_state.clone();
        let snapshot_handle = snapshot_handle.clone();
        let storage_root_snap = storage_root.clone();
        let interval = std::time::Duration::from_secs(snapshot_generation_interval_hours * 3600);
        tokio::spawn(async move {
            let content_storage = match catalyrst_storage::ContentStorage::new(&storage_root_snap)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to initialize content storage for snapshot generation");
                    return;
                }
            };

            loop {
                let state_str = sync_state.get_state();
                if state_str == "Syncing" {
                    tracing::info!("Sync state is Syncing, generating time-windowed snapshots...");
                    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
                    match catalyrst_db::snapshot_generator::generate_snapshots_multi(
                        &pool,
                        &content_storage,
                        catalyrst_db::snapshot_generator::SNAPSHOTS_INIT_TIMESTAMP_MS,
                        now_ms,
                    )
                    .await
                    {
                        Ok(metadatas) => {
                            let snap_json = snapshots_metadata_to_json(&metadatas);
                            let mut handle = snapshot_handle.write().await;
                            *handle = Some(snap_json);
                            tracing::info!(
                                count = metadatas.len(),
                                "Snapshot generation complete, endpoint updated"
                            );
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Snapshot generation failed");
                        }
                    }
                } else {
                    tracing::info!(state = %state_str, "Waiting for Syncing state before generating snapshots");
                }

                tokio::time::sleep(interval).await;
            }
        });
    }

    let bind_addr = format!("{}:{}", host, port);
    tracing::info!(addr = %bind_addr, "catalyrst-live listening");

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| {
            format!("failed to bind {bind_addr} (from HTTP_SERVER_HOST + HTTP_SERVER_PORT)")
        })?;

    use tower::Layer as _;
    let app = tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash().layer(app);
    axum::serve(
        listener,
        axum::ServiceExt::<axum::extract::Request>::into_make_service(app),
    )
    .await
    .context("server error")?;
    Ok(())
}

#[cfg(test)]
mod sync_status_tests {
    use super::*;

    #[test]
    fn gauges_surface_only_after_first_write() {
        let gauges = catalyrst_server::sync_backends::SyncGauges::default();
        let state = LiveSynchronizationState::with_sync_state(
            Arc::new(tokio::sync::RwLock::new(catalyrst_sync::SyncState::Syncing)),
            None,
            gauges.clone(),
        );
        assert_eq!(state.sync_frontier_ms(), None);
        assert_eq!(state.sync_heartbeat_ms(), None);
        gauges
            .frontier_ms
            .store(1234, std::sync::atomic::Ordering::Relaxed);
        gauges
            .heartbeat_ms
            .store(5678, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(state.sync_frontier_ms(), Some(1234));
        assert_eq!(state.sync_heartbeat_ms(), Some(5678));
        assert_eq!(LiveSynchronizationState::new().sync_frontier_ms(), None);
        assert_eq!(LiveSynchronizationState::new().sync_heartbeat_ms(), None);
    }
}
