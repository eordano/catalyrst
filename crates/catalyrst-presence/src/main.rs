use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::{Parser, Subcommand};
use tower_http::trace::TraceLayer;

use catalyrst_presence::config::Config;
use catalyrst_presence::ports::collector::{Collector, SnapshotSummary};
use catalyrst_presence::{api_router, build_collector, build_state, handlers};

const ENV_HELP: &str = "environment variables:
  HTTP_SERVER_HOST                              bind address (default 127.0.0.1)
  HTTP_SERVER_PORT                              listen port (default 5152)
  PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING  required — presence Postgres connection string
  ARCHIPELAGO_URL                               archipelago base URL (default http://127.0.0.1:5139)
  COMMS_URL                                     comms base URL (default http://127.0.0.1:5138)
  WORLDS_SERVER_URL                             worlds content server (default https://worlds-content-server.decentraland.org)
  PRESENCE_GENESIS_REALM                        genesis realm name (default main)
  PRESENCE_SNAPSHOT_INTERVAL_SECS               snapshot interval in seconds for `run` (default 300)
  RUST_LOG                                      tracing filter (default catalyrst_presence=info,tower_http=info)";

#[derive(Parser)]
#[command(
    name = "catalyrst-presence",
    version,
    about = "Unified user-count history collector + read API",
    after_help = ENV_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Snapshot,

    Run {
        #[arg(long)]
        interval: Option<u64>,
    },

    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_presence=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;

    match cli.command {
        Command::Snapshot => {
            let collector = build_collector(&cfg).await?;
            let summary = collector.snapshot().await?;
            print_summary(&summary);
        }
        Command::Run { interval } => {
            let secs = interval.unwrap_or(cfg.snapshot_interval_secs).max(1);
            run_daemon(&cfg, secs).await?;
        }
        Command::Serve => {
            serve(&cfg).await?;
        }
    }
    Ok(())
}

fn print_summary(s: &SnapshotSummary) {
    println!(
        "snapshot #{}: {} peers, {} islands, {} hot scenes | \
         genesis: {} scenes / {} users | \
         worlds: {} polled / {} active / {} users",
        s.snapshot_id,
        s.peers,
        s.islands,
        s.hot_scenes,
        s.scenes_polled,
        s.scene_users,
        s.worlds_polled,
        s.active_worlds,
        s.world_users,
    );
}

async fn build_app_listener(cfg: &Config) -> Result<(Router, tokio::net::TcpListener)> {
    let state = build_state(cfg).await?;
    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "catalyrst-presence listening");
    Ok((app, listener))
}

async fn serve(cfg: &Config) -> Result<()> {
    let (app, listener) = build_app_listener(cfg).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_daemon(cfg: &Config, interval_secs: u64) -> Result<()> {
    let state = build_state(cfg).await?;
    let collector = state.collector.clone();

    let app = Router::new()
        .route("/health", get(handlers::health::health))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, interval_secs, "catalyrst-presence daemon listening");

    let collector_task = tokio::spawn(collector_loop(collector, interval_secs));
    let serve_res = axum::serve(listener, app).await;
    collector_task.abort();
    serve_res?;
    Ok(())
}

async fn collector_loop(collector: Collector, interval_secs: u64) {
    let mut last_aggregated: Option<chrono::NaiveDate> = None;
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        match collector.snapshot().await {
            Ok(s) => tracing::info!(
                snapshot_id = s.snapshot_id,
                peers = s.peers,
                hot_scenes = s.hot_scenes,
                scene_users = s.scene_users,
                world_users = s.world_users,
                "snapshot complete"
            ),
            Err(e) => tracing::error!(error = %e, "snapshot failed; retrying next tick"),
        }

        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).date_naive();
        if last_aggregated != Some(yesterday) {
            match collector.aggregate_day(yesterday).await {
                Ok(()) => last_aggregated = Some(yesterday),
                Err(e) => {
                    tracing::error!(error = %e, date = %yesterday, "daily aggregation failed")
                }
            }
        }
    }
}
