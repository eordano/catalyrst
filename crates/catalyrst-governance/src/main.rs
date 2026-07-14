use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::{Parser, Subcommand};
use tower_http::trace::TraceLayer;

use catalyrst_governance::config::Config;
use catalyrst_governance::{
    api_router, build_client, build_snapshot_gate, build_state, handlers, spawn_sync_loop, sync,
    write_router,
};

const ENV_HELP: &str = "environment variables:
  HTTP_SERVER_HOST                              bind address (default 127.0.0.1)
  HTTP_SERVER_PORT                              listen port (default 5151)
  GOVERNANCE_PG_COMPONENT_PSQL_CONNECTION_STRING  required \u{2014} governance Postgres connection string
  GOVERNANCE_API_URL                            upstream governance API (REQUIRED; no default)
  GOVERNANCE_POLL_ENABLED                       bool \u{2014} start the background sync loop under serve (default false)
  GOVERNANCE_SYNC_WINDOW_HOURS                  sync window in hours (default 48)
  SNAPSHOT_DATABASE_URL                         optional \u{2014} snapshot archive Postgres connection string
  DISCOURSE_DATABASE_URL                        optional \u{2014} discourse archive Postgres connection string
  SNAPSHOT_PRIVATE_KEY                          required for POST /proposals/{type} \u{2014} DAO snapshot poster key
  SNAPSHOT_ADDRESS                              snapshot poster address (default: derived from the key)
  SNAPSHOT_SPACE                                required for POST /proposals/{type} \u{2014} snapshot space id
  SNAPSHOT_API                                  required for POST /proposals/{type} \u{2014} snapshot hub/sequencer URL
  SNAPSHOT_BLOCK_RPC_URL                        required for POST /proposals/{type} \u{2014} eth JSON-RPC for the vp block
  SNAPSHOT_SPACE_COUNCIL                        council space id, required by council-decision-veto
  SNAPSHOT_WEB_URL                              snapshot web UI base (default https://snapshot.org)
  GOVERNANCE_PUBLIC_URL                         governance dApp base (default https://decentraland.org/governance)
  GATSBY_SNAPSHOT_DURATION                      voting window in seconds (default 604800)
  GATSBY_DURATION_GOVERNANCE                    voting window override for governance proposals
  GATSBY_DURATION_HIRING                        voting window override for hiring proposals
  DURATION_TENDER                               voting window override for tender proposals
  DURATION_COUNCIL_DECISION_VETO                voting window override for council-veto proposals
  SUBMISSION_WINDOW_DURATION_TENDER             delay before a tender starts voting, in seconds (default 0)
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
    catalyrst_envcfg::init_tracing("catalyrst_governance=info,tower_http=info");

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
        .with_state(state)
        .merge(write_router(build_snapshot_gate(&cfg)))
        .layer(TraceLayer::new_for_http());

    catalyrst_envcfg::run_service("catalyrst-governance", host, port, app).await
}
