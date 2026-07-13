use anyhow::{Context as _, Result};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::trace::TraceLayer;

use catalyrst_social_rpc::config::Config;
use catalyrst_social_rpc::db::Db;
use catalyrst_social_rpc::profiles::Profiles;
use catalyrst_social_rpc::state::AppStateInner;
use catalyrst_social_rpc::ws::ws_upgrade;

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5148)"),
    (
        "AUTH_WINDOW_SECS",
        "signed-fetch auth window in seconds (default 300)",
    ),
    (
        "DATABASE_URL",
        "required — social-rpc Postgres connection string",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (default http://127.0.0.1:5138)",
    ),
    (
        "COMMS_GATEKEEPER_AUTH_TOKEN",
        "optional — bearer token for the comms gatekeeper",
    ),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional — catalyst content DB for profile enrichment",
    ),
    (
        "CONTENT_SERVER_ADDRESS",
        "content server base URL (default https://peer.decentraland.org/content)",
    ),
    (
        "PRIVATE_VOICE_CHAT_EXPIRATION_TIME",
        "private voice chat expiration in milliseconds (default 60000)",
    ),
    (
        "PRIVATE_VOICE_CHAT_JOB_INTERVAL",
        "private voice chat expiration job interval in milliseconds (default 1000)",
    ),
    (
        "PRIVATE_VOICE_CHAT_EXPIRATION_BATCH_SIZE",
        "private voice chat expiration batch size (default 20)",
    ),
    (
        "WS_MAX_CONCURRENT_CONNECTIONS",
        "optional — cap on concurrent WebSocket connections (unset = unlimited)",
    ),
    (
        "WS_MAX_PAYLOAD_LENGTH",
        "maximum inbound WebSocket message size in bytes (default 1048576)",
    ),
    (
        "CATALYRST_SOCIAL_RPC_ADMIN_TOKEN",
        "optional — bearer token guarding /admin/social",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_social_rpc=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-social-rpc", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_social_rpc=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect to social-rpc database")?;

    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        tracing::error!(error = %e, "migration failed");
        return Err(e.into());
    }

    let content_pool = match &cfg.content_database_url {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect(url)
            .await
        {
            Ok(p) => {
                tracing::info!("connected to content DB for profile enrichment");
                Some(p)
            }
            Err(e) => {
                tracing::warn!(error = %e, "content DB unavailable; profile enrichment disabled");
                None
            }
        },
        None => {
            tracing::info!("CONTENT_PG_CONNECTION_STRING unset; profile enrichment disabled");
            None
        }
    };

    let db = Db::new(pool);
    let profiles = Profiles::new(content_pool, cfg.content_server_address.clone());
    let state: Arc<AppStateInner> = Arc::new(AppStateInner::new(cfg.clone(), db, profiles));
    state.init_rpc().await;

    let app = Router::new()
        .route("/", get(ws_upgrade))
        .route("/info", get(root))
        .route("/health", get(health))
        .route("/health/live", get(health_live))
        .nest("/admin/social", catalyrst_social_rpc::admin::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-social-rpc listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> Json<serde_json::Value> {
    Json(json!({
        "service": "catalyrst-social-rpc",
        "version": env!("CARGO_PKG_VERSION"),
        "ws": "/",
    }))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "healthy": true }))
}

async fn health_live() -> &'static str {
    "alive"
}
