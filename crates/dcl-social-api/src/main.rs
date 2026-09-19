use dcl_social_api::{app, Store};
use std::sync::Arc;

const ENV_DOCS: &[(&str, &str)] = &[
    ("SOCIAL_BIND", "listen address (default 127.0.0.1:5191)"),
    (
        "SOCIAL_DATABASE",
        "SQLite database (default dcl-social.sqlite)",
    ),
    (
        "SOCIAL_ASSETS",
        "built browser assets (default social/dist)",
    ),
    (
        "SOCIAL_UPSTREAM",
        "Foundation social API origin (default https://social-api.decentraland.org; https unless SOCIAL_ALLOW_LOCAL_UPSTREAM=1)",
    ),
    (
        "SOCIAL_ALLOW_LOCAL_UPSTREAM",
        "set to 1 to accept a plain-http localhost/127.0.0.1 SOCIAL_UPSTREAM (dev only)",
    ),
    (
        "SOCIAL_RELEASE",
        "release label on telemetry events (default: the releases/<name>/bin parent dir when launched from one, else dcl-social-<crate version>)",
    ),
    (
        "SOCIAL_TELEMETRY_URL",
        "store endpoint for error events (default https://interconnected.online/telemetry/api/dcl-social/store/)",
    ),
    ("RUST_LOG", "tracing filter"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    catalyrst_envcfg::handle_standard_args_with_version(
        "dcl-social-api",
        env!("CARGO_PKG_VERSION"),
        ENV_DOCS,
    );
    dcl_social_api::telemetry::init();
    let result = run().await;
    if let Err(error) = &result {
        tracing::error!("social startup or runtime failure: {error}");
    }
    dcl_social_api::telemetry::flush().await;
    result
}
async fn run() -> anyhow::Result<()> {
    let cfg = dcl_social_api::config::Config::from_env()?;
    let state = Arc::new(Store::open(
        &cfg.database,
        cfg.upstream.trim_end_matches('/'),
    )?);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, "dcl.social listening");
    axum::serve(listener, app(state, cfg.assets))
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("register SIGTERM");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
        })
        .await?;
    Ok(())
}
