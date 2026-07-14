use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_badges::config::Config;
use catalyrst_badges::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5147)"),
    (
        "BADGES_PG_CONNECTION_STRING",
        "required — badges Postgres connection string",
    ),
    (
        "CATALYRST_BADGES_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_badges=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-badges", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_badges=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-badges", cfg.http_host, cfg.http_port, app).await
}
