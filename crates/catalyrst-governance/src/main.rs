use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_governance::config::Config;
use catalyrst_governance::{
    api_router, build_client, build_state, handlers, spawn_sync_loop, sync,
};

const ENV_HELP: &str = "environment variables:
  HTTP_SERVER_HOST                              bind address (default 127.0.0.1)
  HTTP_SERVER_PORT                              listen port (default 5151)
  GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING  required — governance Postgres connection string
  GOVERNANCE_API_URL                            upstream governance API (default https://governance.decentraland.org/api)
  GOVERNANCE_POLL_ENABLED                       bool — start the background sync loop under serve (default false)
  GOVERNANCE_SYNC_WINDOW_HOURS                  sync window in hours (default 48)
  SNAPSHOT_DATABASE_URL                         optional — snapshot archive Postgres connection string
  DISCOURSE_DATABASE_URL                        optional — discourse archive Postgres connection string
  RUST_LOG                                      tracing filter (default catalyrst_governance=info,tower_http=info)";

#[derive(Parser)]
#[command(
    name = "catalyrst-governance",
    version,
    about = "Governance archive + read API",
    after_help = ENV_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Serve,

    Backfill,

    Sync {
        #[arg(long)]
        window: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_governance=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let cfg = Config::from_env()?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(cfg).await,
        Command::Backfill => {
            let state = build_state(&cfg).await?;
            let client = build_client(&cfg)?;
            sync::backfill(&client, &state.store).await
        }
        Command::Sync { window } => {
            let state = build_state(&cfg).await?;
            let client = build_client(&cfg)?;
            let window = window.unwrap_or(cfg.sync_window_hours);
            sync::sync(&client, &state.store, window).await
        }
    }
}

async fn serve(cfg: Config) -> Result<()> {
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(&cfg).await?;

    if cfg.poll_enabled {
        let client = build_client(&cfg)?;
        spawn_sync_loop(state.clone(), client, cfg.sync_window_hours);
    } else {
        tracing::info!("GOVERNANCE_POLL_ENABLED is false; background sync loop not started");
    }

    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-governance listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
