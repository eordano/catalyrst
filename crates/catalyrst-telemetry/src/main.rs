use anyhow::Result;
use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use catalyrst_telemetry::{api_router, build_state, Config};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5150)"),
    (
        "TELEMETRY_PG_CONNECTION_STRING",
        "required \u{2014} telemetry Postgres connection string",
    ),
    (
        "CATALYRST_TELEMETRY_ADMIN_TOKEN",
        "optional \u{2014} bearer token guarding the admin endpoints",
    ),
    (
        "TELEMETRY_CONTRACT_PATH",
        "optional \u{2014} path to telemetry-contract.json; enables ingest-side contract validation (unset/missing/unparseable = validation disabled, accept all)",
    ),
    (
        "FLAGS_URL",
        "feature-flags source for /dash/flags (default http://127.0.0.1:5137/explorer.json)",
    ),
    (
        "TELEMETRY_BASE_PATH",
        "URL prefix the dashboard is served under when nginx strips it (e.g. /telemetry; default empty = root)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_telemetry=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-telemetry", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_telemetry=info,tower_http=info");

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(api_router(state.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    catalyrst_envcfg::run_service("catalyrst-telemetry", &cfg.http_host, cfg.http_port, app).await
}
