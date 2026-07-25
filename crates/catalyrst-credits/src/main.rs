use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_credits::config::Config;
use catalyrst_credits::handlers;
use catalyrst_credits::{api_router, build_state};

const ENV_DOCS: &[(&str, &str)] = &[
    (
        "HTTP_SERVER_HOST",
        "bind address (default 127.0.0.1; non-loopback refuses to start without CATALYRST_CREDITS_ADMIN_TOKEN)",
    ),
    ("HTTP_SERVER_PORT", "listen port (default 5150)"),
    (
        "CREDITS_PG_CONNECTION_STRING",
        "required — credits Postgres connection string",
    ),
    (
        "CATALYRST_CREDITS_ADMIN_TOKEN",
        "optional — bearer token guarding the admin money endpoints",
    ),
    ("CREDITS_CAPTCHA_SECRET", "optional — hcaptcha secret"),
    (
        "CREDITS_CAPTCHA_VERIFY_URL",
        "captcha verify endpoint (default https://hcaptcha.com/siteverify)",
    ),
    (
        "STRIPE_SECRET_KEY",
        "optional — enables the Stripe client",
    ),
    (
        "STRIPE_WEBHOOK_SECRET",
        "optional — Stripe webhook signature secret",
    ),
    (
        "STRIPE_API_BASE",
        "Stripe API base URL (default https://api.stripe.com)",
    ),
    ("CREDITS_CURRENCY", "checkout currency (default usd)"),
    (
        "MARKET_BASE_URL",
        "catalyrst-market base URL (default http://127.0.0.1:5133)",
    ),
    (
        "PRICE_BASE_URL",
        "price service base URL (default http://127.0.0.1:5156)",
    ),
    (
        "ECONOMY_BASE_URL",
        "economy service base URL (default http://127.0.0.1:5155)",
    ),
    (
        "CATALYRST_ECONOMY_ADMIN_TOKEN",
        "optional — bearer token for economy admin calls",
    ),
    (
        "MARKETPLACE_MARKUP_BPS",
        "marketplace markup in basis points (default 2500)",
    ),
    (
        "MANA_PRICE_MAX_STALENESS_SECS",
        "max MANA price staleness in seconds (default 300)",
    ),
    (
        "CHECKOUT_FULFILLMENT_MODE",
        "secondary | primary | auto (default secondary)",
    ),
    (
        "CREDITS_REQUIRE_PURCHASE_INTENT",
        "bool — require a purchase intent before checkout (default true)",
    ),
    (
        "LANDILER_ESCROW_ADDRESS",
        "optional — LandilerEscrow contract address",
    ),
    (
        "CHECKOUT_WORKER_INTERVAL_SECS",
        "checkout worker poll interval in seconds (default 5)",
    ),
    (
        "CHECKOUT_MAX_ATTEMPTS",
        "checkout fulfillment attempt cap (default 5)",
    ),
    (
        "USAGE_GRANTS_PG_CONNECTION_STRING",
        "optional — usage-grants Postgres connection string",
    ),
    (
        "PROGRESS_PRESENCE_PG_CONNECTION_STRING",
        "optional — progress/presence Postgres connection string",
    ),
    (
        "ESCROW_LOCK_DAYS",
        "escrow lock duration in days (default 15)",
    ),
    (
        "CREDITS_MOCK_FULFILLMENT",
        "bool — mock fulfillment (default false)",
    ),
    (
        "CREDITS_MOCK_CARD",
        "bool — mock card payments (default false)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_credits=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-credits", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_credits=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let host = cfg.http_host.clone();
    let port = cfg.http_port;

    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::ping::ping))
        .route("/health/live", get(|| async { "alive" }))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(%addr, "catalyrst-credits listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
