use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_communities::config::Config;
use catalyrst_communities::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 8080)"),
    (
        "COMMUNITIES_PG_CONNECTION_STRING",
        "required — communities Postgres connection string",
    ),
    (
        "API_ADMIN_TOKEN",
        "optional — bearer token guarding admin endpoints",
    ),
    (
        "COMMUNITIES_CONTENT_DIR",
        "community content directory (default ./data/communities/content)",
    ),
    (
        "MUTES_PG_CONNECTION_STRING",
        "optional — mutes Postgres connection string",
    ),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional — catalyst content DB connection string",
    ),
    (
        "CONTENT_SERVER_ADDRESS",
        "content server base URL (default https://peer.decentraland.org/content/)",
    ),
    (
        "CDN_URL",
        "CDN base URL (default https://cdn.decentraland.org)",
    ),
    (
        "COMMUNITIES_GLOBAL_MODERATORS",
        "comma-separated global moderator addresses",
    ),
    ("PLACES_API_URL", "optional — places API base URL"),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_communities=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-communities", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_communities=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-communities listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
