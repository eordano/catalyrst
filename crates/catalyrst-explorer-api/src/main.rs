use std::net::SocketAddr;

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
        "catalyst content server base URL (default http://127.0.0.1:5140)",
    ),
    (
        "LAMBDAS_URL",
        "lambdas base URL (default http://127.0.0.1:5142)",
    ),
    (
        "COMMS_URL",
        "comms base URL (default http://127.0.0.1:5137/comms)",
    ),
    (
        "UPSTREAM_MARKETPLACE_URL",
        "upstream marketplace API (default https://marketplace-api.decentraland.org)",
    ),
    (
        "UPSTREAM_BUILDER_URL",
        "upstream builder API (default https://builder-api.decentraland.org)",
    ),
    (
        "UPSTREAM_WORLDS_URL",
        "upstream worlds-content-server (default https://worlds-content-server.decentraland.org)",
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
    ("ONBOARDING_API_KEY", "optional — onboarding API key"),
    (
        "CATALYRST_EXPLORER_API_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "MAP_SATELLITE_BASE_URL",
        "minimap satellite tiles base URL (default https://genesis.city/map/latest)",
    ),
    (
        "MAP_PARCEL_VIEW_URL",
        "minimap parcel view image URL (default https://api.decentraland.org/v1/minimap.png)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_explorer_api=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-explorer-api", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_explorer_api=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .merge(modules::ping::routes())
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-explorer-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
