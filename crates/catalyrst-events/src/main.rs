use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_events::config::Config;
use catalyrst_events::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5135)"),
    (
        "PLACES_EVENTS_PG_CONNECTION_STRING",
        "required \u{2014} places_events Postgres connection string",
    ),
    (
        "CATALYRST_EVENTS_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
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
        "EVENTS_MIRROR_UPSTREAM",
        "bool \u{2014} mirror the event catalog from an upstream events API into the event table",
    ),
    (
        "EVENTS_UPSTREAM_URL",
        "upstream events API base for the mirror (default https://events.decentraland.org)",
    ),
    (
        "EVENTS_MIRROR_INTERVAL_SECS",
        "seconds between mirror passes (default 3600)",
    ),
    (
        "HTTP_BASE_URL",
        "public base URL whose domain rewrites mirrored asset links (unset = no rewrite)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_events=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-events", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_events=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-events", cfg.http_host, cfg.http_port, app).await
}
