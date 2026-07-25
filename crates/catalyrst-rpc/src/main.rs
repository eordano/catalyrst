use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_rpc::config::Config;
use catalyrst_rpc::modules;
use catalyrst_rpc::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5153)"),
    (
        "CATALYRST_RPC_ADMIN_TOKEN",
        "optional — bearer token for /admin/rpc/*; unset fails closed (403)",
    ),
    (
        "RPC_UPSTREAM_MAINNET",
        "upstream for mainnet (default https://rpc.decentraland.org/mainnet)",
    ),
    (
        "RPC_UPSTREAM_ETHEREUM",
        "upstream for ethereum (default https://rpc.decentraland.org/mainnet)",
    ),
    (
        "RPC_UPSTREAM_SEPOLIA",
        "upstream for sepolia (default https://rpc.decentraland.org/sepolia)",
    ),
    (
        "RPC_UPSTREAM_POLYGON",
        "upstream for polygon (default https://rpc.decentraland.org/polygon)",
    ),
    (
        "RPC_UPSTREAM_MATIC",
        "upstream for matic (default https://rpc.decentraland.org/polygon)",
    ),
    (
        "RPC_UPSTREAM_AMOY",
        "upstream for amoy (default https://rpc.decentraland.org/amoy)",
    ),
    (
        "RPC_UPSTREAM_MUMBAI",
        "upstream for mumbai (default https://rpc.decentraland.org/mumbai)",
    ),
    (
        "RPC_UPSTREAM_ARBITRUM",
        "upstream for arbitrum (default https://rpc.decentraland.org/arbitrum)",
    ),
    (
        "RPC_UPSTREAM_OPTIMISM",
        "upstream for optimism (default https://rpc.decentraland.org/optimism)",
    ),
    (
        "RPC_UPSTREAM_AVALANCHE",
        "upstream for avalanche (default https://rpc.decentraland.org/avalanche)",
    ),
    (
        "RPC_UPSTREAM_BINANCE",
        "upstream for binance (default https://rpc.decentraland.org/binance)",
    ),
    (
        "RPC_UPSTREAM_FANTOM",
        "upstream for fantom (default https://rpc.decentraland.org/fantom)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_rpc=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-rpc", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_rpc=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(cfg).await?;

    let app = Router::new()
        .merge(modules::ping::routes())
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-rpc listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
