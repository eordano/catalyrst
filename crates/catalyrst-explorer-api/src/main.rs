use anyhow::Result;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_explorer_api::config::Config;
use catalyrst_explorer_api::{api_router, build_state, modules};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5137)"),
    ("REALM_NAME", "realm name (default catalyrst)"),
    (
        "CATALYST_URL",
        "catalyst content server base URL (default http://127.0.0.1:5141)",
    ),
    (
        "LAMBDAS_URL",
        "lambdas base URL (default http://127.0.0.1:5141/lambdas)",
    ),
    (
        "COMMS_URL",
        "comms base URL (default http://127.0.0.1:5137/comms)",
    ),
    (
        "UPSTREAM_MARKETPLACE_URL",
        "upstream marketplace API (default http://127.0.0.1:5133)",
    ),
    (
        "UPSTREAM_BUILDER_URL",
        "upstream builder API (default http://127.0.0.1:5144)",
    ),
    (
        "UPSTREAM_WORLDS_URL",
        "upstream worlds-content-server (default http://127.0.0.1:5142)",
    ),
    (
        "UPSTREAM_WORLDS_CONTENT_URL",
        "worlds content base URL (falls back to WORLDS_URL, then http://127.0.0.1:5142)",
    ),
    ("NETWORK_ID", "ethereum network id (default 1)"),
    ("ENV_NAME", "environment name (default prd)"),
    (
        "PUBLIC_REALM_URL",
        "public realm URL (default http://127.0.0.1:5137)",
    ),
    ("BFF_URL", "bff URL (default /bff)"),
    ("COMMS_ADAPTER", "comms adapter (default offline:offline)"),
    (
        "COMMS_FIXED_ADAPTER",
        "comms fixed adapter (default offline:offline)",
    ),
    (
        "FEATURE_FLAGS_CONFIG_PATH",
        "feature flags JSON path (default ./config/feature-flags.json)",
    ),
    (
        "BLOCKLIST_PATH",
        "denylist JSON path (default ./config/denylist.json)",
    ),
    (
        "HOT_SCENES_URL",
        "hot scenes URL (default http://127.0.0.1:5143/hot-scenes)",
    ),
    ("ONBOARDING_API_KEY", "optional \u{2014} onboarding API key"),
    (
        "CATALYRST_EXPLORER_API_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
    ),
    (
        "MAP_SATELLITE_BASE_URL",
        "minimap satellite tiles base URL (default http://127.0.0.1:5162/satellite)",
    ),
    (
        "MAP_PARCEL_VIEW_URL",
        "minimap parcel view image URL (default http://127.0.0.1:5162/v1/minimap.png)",
    ),
    (
        "HTTP_BASE_URL",
        "public base URL for self-referencing links, trailing slash stripped (unset = relative)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_explorer_api=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-explorer-api", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_explorer_api=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .merge(modules::ping::routes())
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-explorer-api", cfg.http_host, cfg.http_port, app).await
}
