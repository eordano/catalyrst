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

#[tokio::main]
async fn main() -> Result<()> {
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
