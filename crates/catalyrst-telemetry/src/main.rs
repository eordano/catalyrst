use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_telemetry::{api_router, build_state, Config};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5150)"),
    (
        "TELEMETRY_PG_CONNECTION_STRING",
        "required — telemetry Postgres connection string",
    ),
    (
        "CATALYRST_TELEMETRY_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "FLAGS_URL",
        "feature-flags source for /dash/flags (default http://127.0.0.1:5137/explorer.json)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_telemetry=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-telemetry", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_telemetry=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-telemetry listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
