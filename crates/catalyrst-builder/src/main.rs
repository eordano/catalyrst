use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use catalyrst_builder::config::Config;
use catalyrst_builder::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5145)"),
    (
        "BUILDER_PG_CONNECTION_STRING",
        "required — builder Postgres connection string",
    ),
    (
        "BUILDER_MARKETPLACE_PG_CONNECTION_STRING",
        "optional — marketplace Postgres connection string",
    ),
    (
        "BUILDER_CONTENT_BUCKET_URL",
        "item content bucket base URL (default https://builder-items.decentraland.org)",
    ),
    (
        "BUILDER_ADMIN_ADDRESSES",
        "comma-separated admin wallet addresses (lowercased)",
    ),
    (
        "NEWSLETTER_SERVICE_URL",
        "optional — newsletter service base URL",
    ),
    (
        "NEWSLETTER_PUBLICATION_ID",
        "optional — newsletter publication id",
    ),
    (
        "NEWSLETTER_SERVICE_API_KEY",
        "optional — newsletter service API key",
    ),
    (
        "CATALYRST_BUILDER_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_builder=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-builder", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_builder=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-builder listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
