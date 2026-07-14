use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_social_service::rest::config::Config;
use catalyrst_social_service::rest::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 8080)"),
    (
        "COMMUNITIES_PG_CONNECTION_STRING",
        "required \u{2014} communities Postgres connection string",
    ),
    (
        "API_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding admin endpoints",
    ),
    (
        "COMMUNITIES_CONTENT_DIR",
        "community content directory (default ./data/communities/content)",
    ),
    (
        "MUTES_PG_CONNECTION_STRING",
        "optional \u{2014} mutes Postgres connection string",
    ),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional \u{2014} catalyst content DB connection string",
    ),
    (
        "CONTENT_SERVER_ADDRESS",
        "content server base URL (default http://127.0.0.1:5141)",
    ),
    ("CDN_URL", "CDN base URL (REQUIRED; no default)"),
    (
        "COMMUNITIES_GLOBAL_MODERATORS",
        "comma-separated global moderator addresses",
    ),
    ("PLACES_API_URL", "optional \u{2014} places API base URL"),
    (
        "ARCHIPELAGO_STATS_URL",
        "archipelago stats base URL for onlyOnline member presence (default http://127.0.0.1:5139)",
    ),
    (
        "RESTRICTED_NAMES",
        "comma-separated denylist of community names, compared case/space-folded (default empty)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_social_service=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-communities", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_social_service=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health/live", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-communities", cfg.http_host, cfg.http_port, app).await
}
