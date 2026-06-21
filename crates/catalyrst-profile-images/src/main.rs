use anyhow::Result;
use axum::http::Method;
use axum::routing::get;
use axum::{Json, Router};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use catalyrst_profile_images::config::Config;
use catalyrst_profile_images::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5152)"),
    (
        "PROFILE_IMAGES_BACKEND",
        "render | proxy | disabled (default: render if PROFILE_IMAGES_CONTENT_URL set, else proxy if PROFILE_IMAGES_ORIGIN_URL set, else disabled)",
    ),
    (
        "PROFILE_IMAGES_CONTENT_URL",
        "content server base for the render backend (e.g. http://127.0.0.1:5141/content)",
    ),
    (
        "PROFILE_IMAGES_ORIGIN_URL",
        "upstream profile-images service for the proxy backend / render fallback",
    ),
    (
        "PROFILE_IMAGES_CACHE_DIR",
        "on-disk image cache directory (default ./data/profile-images)",
    ),
    (
        "PROFILE_IMAGES_CACHE_TTL_SECONDS",
        "cache TTL in seconds (default 86400)",
    ),
    (
        "PROFILE_IMAGES_RENDER_FALLBACK_PROXY",
        "bool — proxy to PROFILE_IMAGES_ORIGIN_URL when a render fails (default false)",
    ),
    (
        "PROFILE_IMAGES_GODOT_BIN",
        "required for render backend — path to decentraland.godot.client.x86_64",
    ),
    (
        "PROFILE_IMAGES_GODOT_PROJECT",
        "godot project root (default: derived from the godot bin path)",
    ),
    (
        "PROFILE_IMAGES_RENDERING_METHOD",
        "godot rendering method (default gl_compatibility)",
    ),
    (
        "PROFILE_IMAGES_RENDERING_DRIVER",
        "godot rendering driver (default opengl3)",
    ),
    (
        "PROFILE_IMAGES_DCLENV",
        "optional — DCLENV value passed to the renderer",
    ),
    (
        "PROFILE_IMAGES_GODOT_HEADLESS",
        "bool — run godot with --headless (default false)",
    ),
    (
        "PROFILE_IMAGES_GODOT_DISPLAY",
        "optional — DISPLAY value for the renderer",
    ),
    (
        "PROFILE_IMAGES_GODOT_EXTRA_ARGS",
        "optional — extra whitespace-separated godot args",
    ),
    (
        "PROFILE_IMAGES_RENDER_TIMEOUT_SECONDS",
        "per-render timeout in seconds (default 120)",
    ),
    (
        "PROFILE_IMAGES_RENDER_MAX_CONCURRENT",
        "max concurrent renders (default 1)",
    ),
    (
        "PROFILE_IMAGES_RENDER_WORKDIR",
        "render scratch dir (default CACHE_DIR/.render-tmp)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_profile_images=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-profile-images", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_profile_images=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods([Method::GET]);

    let app = Router::new()
        .route("/health", get(|| async { Json("alive") }))
        .route("/health/live", get(|| async { Json("alive") }))
        .merge(api_router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, backend = cfg.backend_kind.label(), cache = %cfg.cache_dir, "catalyrst-profile-images listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
