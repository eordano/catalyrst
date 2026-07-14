use anyhow::Result;
use axum::routing::get;
use axum::Router;
use catalyrst_envcfg::service_scaffold::finish_app;
use std::net::SocketAddr;

use catalyrst_media::config::Config;
use catalyrst_media::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5157)"),
    (
        "MEDIA_PG_CONNECTION_STRING",
        "required -- media Postgres connection string",
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
        "optional -- API key sent to the translation backend",
    ),
    (
        "TRANSLATE_CHAR_LIMIT",
        "max characters per /translate request, summed across a batch (default 5000)",
    ),
    (
        "TRANSLATE_BATCH_LIMIT",
        "max items per /translate batch (default 100)",
    ),
    (
        "TRANSLATE_REQUEST_TIMEOUT_SECS",
        "deadline over the whole backend call of one /translate request (default 30)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_media=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-media", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_media=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = finish_app(
        Router::new()
            .route("/ping", get(handlers::ping::ping))
            .route("/health", get(handlers::health::health))
            .merge(api_router()),
        state,
        None,
    );

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, backend = cfg.backend_kind.label(), "catalyrst-media (autotranslate) listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
