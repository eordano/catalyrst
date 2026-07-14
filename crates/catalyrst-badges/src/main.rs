use anyhow::Result;
use tower_http::trace::TraceLayer;

use catalyrst_badges::config::Config;
use catalyrst_badges::{app_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5147)"),
    (
        "BADGES_PG_CONNECTION_STRING",
        "required \u{2014} badges Postgres connection string",
    ),
    (
        "CATALYRST_BADGES_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
    ),
    (
        "BADGES_ASSETS_DIR",
        "optional \u{2014} directory served at /assets (default ./assets); missing files 404, missing dir doesn't crash startup",
    ),
    (
        "BADGES_PUBLIC_ASSET_BASE_URL",
        "optional \u{2014} host+scheme substituted for https://badges.decentraland.org in every assets URL at serve time (default: unchanged, keeps upstream parity)",
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

    let app = app_router(&cfg)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-badges", cfg.http_host, cfg.http_port, app).await
}
