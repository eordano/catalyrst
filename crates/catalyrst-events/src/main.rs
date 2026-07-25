use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_events::config::Config;
use catalyrst_events::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5135)"),
    (
        "PLACES_EVENTS_PG_CONNECTION_STRING",
        "required — places_events Postgres connection string",
    ),
    (
        "CATALYRST_EVENTS_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "CATALYRST_EVENTS_CONTENT_DIR",
        "content store directory (default /tmp/catalyrst-events-content)",
    ),
    (
        "COMMS_GATEKEEPER_URL",
        "comms gatekeeper base URL (default http://127.0.0.1:5138)",
    ),
    (
        "EVENTS_BASE_URL",
        "public base URL used in sitemap links (default https://events.decentraland.org)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_events=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-events", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_events=info,tower_http=info".into()),
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
    tracing::info!(%addr, "catalyrst-events listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
