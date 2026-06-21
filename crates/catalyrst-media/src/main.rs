use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_media::config::Config;
use catalyrst_media::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5157)"),
    (
        "MEDIA_PG_CONNECTION_STRING",
        "required — media Postgres connection string",
    ),
    (
        "TRANSLATE_BACKEND",
        "mock | http (default mock; defaults to http when TRANSLATE_BACKEND_URL is set)",
    ),
    (
        "TRANSLATE_BACKEND_URL",
        "translation backend base URL (required when TRANSLATE_BACKEND=http)",
    ),
    (
        "TRANSLATE_BACKEND_API_KEY",
        "optional — API key sent to the translation backend",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_media=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-media", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_media=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::health::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, backend = cfg.backend_kind.label(), "catalyrst-media (autotranslate) listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
