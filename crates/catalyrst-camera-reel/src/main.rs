use anyhow::Result;
use axum::extract::DefaultBodyLimit;
use axum::http::Method;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use catalyrst_camera_reel::config::Config;
use catalyrst_camera_reel::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5149)"),
    (
        "CAMERA_REEL_PG_CONNECTION_STRING",
        "required \u{2014} camera-reel Postgres connection string",
    ),
    (
        "CONTENT_STORAGE_DIR",
        "image storage directory (default ./data/camera-reel)",
    ),
    (
        "API_URL",
        "public base URL for image links (default http://127.0.0.1:5149)",
    ),
    ("BUCKET_URL", "optional \u{2014} external bucket base URL"),
    ("MAX_IMAGES_PER_USER", "per-user image quota (default 500)"),
    (
        "PLACES_API_URL",
        "places API base URL (default http://127.0.0.1:5134)",
    ),
    (
        "PLACES_CACHE_TTL_SECONDS",
        "places cache TTL in seconds (default 300)",
    ),
    (
        "PLACES_CACHE_MAX_SIZE",
        "places cache max entries (default 1000)",
    ),
    (
        "CATALYRST_CAMERA_REEL_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_camera_reel=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-camera-reel", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_camera_reel=info,tower_http=info");

    let cfg = Config::from_env()?;
    let http_host = cfg.http_host.clone();
    let http_port = cfg.http_port;

    let state = build_state(cfg).await?;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE]);

    let app = Router::new()
        .route("/health", get(|| async { Json("alive") }))
        .route("/health/live", get(|| async { Json("alive") }))
        .merge(api_router())
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-camera-reel", http_host, http_port, app).await
}
