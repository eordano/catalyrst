use std::sync::Arc;

use catalyrst_quests::{build_router, config, db::Db};

const ENV_DOCS: &[(&str, &str)] = &[
    ("QUESTS_BIND", "bind address (default 127.0.0.1:5155)"),
    (
        "QUESTS_DATABASE_URL",
        "optional -- quests Postgres connection string (unset = serve empty)",
    ),
    (
        "QUESTS_AUTH_WINDOW_SECS",
        "signed-fetch auth window in seconds (default 300)",
    ),
    ("RUST_LOG", "tracing filter (default catalyrst_quests=info)"),
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-quests", ENV_DOCS);

    catalyrst_envcfg::init_tracing("catalyrst_quests=info");

    let db = match config::database_url() {
        Some(url) => match Db::connect(&url).await {
            Ok(d) => {
                tracing::info!("quests db connected");
                Some(Arc::new(d))
            }
            Err(e) => {
                tracing::warn!(error = %e, "quests db unavailable; serving empty");
                None
            }
        },
        None => {
            tracing::warn!("QUESTS_DATABASE_URL unset; serving empty");
            None
        }
    };

    let router = build_router(db).await;

    let bind = config::bind_addr();
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "catalyrst-quests listening");
    axum::serve(listener, router).await?;
    Ok(())
}
