use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use catalyrst_commons::worker::{spawn_periodic, PeriodicCfg};
use catalyrst_envcfg::service_scaffold::finish_app;
use clap::{Parser, Subcommand};
use tokio_util::sync::CancellationToken;

use catalyrst_presence::config::Config;
use catalyrst_presence::ports::collector::{Collector, SnapshotSummary};
use catalyrst_presence::{api_router, build_collector, build_state, handlers};

const ENV_HELP: &str = "environment variables:
  HTTP_SERVER_HOST                              bind address (default 127.0.0.1)
  HTTP_SERVER_PORT                              listen port (default 5152)
  PRESENCE_PG_COMPONENT_PSQL_CONNECTION_STRING  required -- presence Postgres connection string
  ARCHIPELAGO_URL                               archipelago base URL (default http://127.0.0.1:5139)
  COMMS_URL                                     comms base URL (default http://127.0.0.1:5138)
  WORLDS_SERVER_URL                             worlds content server (default http://127.0.0.1:5142)
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

    catalyrst_envcfg::init_tracing("catalyrst_presence=info,tower_http=info");

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
    let app = finish_app(
        Router::new()
            .route("/health", get(handlers::health::health))
            .merge(api_router()),
        state.clone(),
        None,
    );

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

    let app = finish_app(
        Router::new()
            .route("/health", get(handlers::health::health))
            .merge(api_router()),
        state.clone(),
        None,
    );

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, interval_secs, "catalyrst-presence daemon listening");

    let shutdown = CancellationToken::new();
    let last_aggregated: Arc<Mutex<Option<chrono::NaiveDate>>> = Arc::new(Mutex::new(None));
    let collector_task = spawn_periodic(
        "presence-collector",
        Duration::from_secs(interval_secs),
        PeriodicCfg::default(),
        shutdown.clone(),
        move || {
            let collector = collector.clone();
            let last_aggregated = last_aggregated.clone();
            async move { collector_pass(&collector, &last_aggregated).await }
        },
    );

    let serve_res = axum::serve(listener, app).await;
    shutdown.cancel();
    let _ = collector_task.await;
    serve_res?;
    Ok(())
}

async fn collector_pass(
    collector: &Collector,
    last_aggregated: &Mutex<Option<chrono::NaiveDate>>,
) -> Result<()> {
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
    let already_aggregated = *last_aggregated.lock().unwrap() == Some(yesterday);
    if !already_aggregated {
        match collector.aggregate_day(yesterday).await {
            Ok(()) => *last_aggregated.lock().unwrap() = Some(yesterday),
            Err(e) => {
                tracing::error!(error = %e, date = %yesterday, "daily aggregation failed")
            }
        }
    }

    Ok(())
}
