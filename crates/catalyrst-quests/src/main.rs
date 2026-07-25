use std::sync::Arc;

use catalyrst_quests::{build_router, config, db::Db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

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
