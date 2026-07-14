use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use catalyrst_builder::config::Config;
use catalyrst_builder::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5145)"),
    (
        "BUILDER_PG_CONNECTION_STRING",
        "required \u{2014} builder Postgres connection string",
    ),
    (
        "BUILDER_MARKETPLACE_PG_CONNECTION_STRING",
        "optional \u{2014} marketplace Postgres connection string",
    ),
    (
        "BUILDER_CONTENT_BUCKET_URL",
        "item content bucket base URL (REQUIRED; no default)",
    ),
    (
        "BUILDER_ADMIN_ADDRESSES",
        "comma-separated admin wallet addresses (lowercased)",
    ),
    (
        "NEWSLETTER_SERVICE_URL",
        "optional \u{2014} newsletter service base URL",
    ),
    (
        "NEWSLETTER_PUBLICATION_ID",
        "optional \u{2014} newsletter publication id",
    ),
    (
        "NEWSLETTER_SERVICE_API_KEY",
        "optional \u{2014} newsletter service API key",
    ),
    (
        "CATALYRST_BUILDER_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_builder=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("catalog") {
        let packs = args
            .get(2)
            .context("usage: catalyrst-builder catalog <packs-dir> <out-dir>")?;
        let out = args
            .get(3)
            .context("usage: catalyrst-builder catalog <packs-dir> <out-dir>")?;
        return catalyrst_builder::catalog_build::run(packs, out);
    }

    catalyrst_envcfg::handle_standard_args("catalyrst-builder", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_builder=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-builder", cfg.http_host, cfg.http_port, app).await
}
