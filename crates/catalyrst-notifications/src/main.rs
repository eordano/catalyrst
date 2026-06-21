use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use catalyrst_notifications::config::Config;
use catalyrst_notifications::{api_router, build_state, handlers};

const ENV_DOCS: &[(&str, &str)] = &[
    ("HTTP_SERVER_HOST", "bind address (default 127.0.0.1)"),
    ("HTTP_SERVER_PORT", "listen port (default 5148)"),
    (
        "NOTIFICATIONS_PG_CONNECTION_STRING",
        "required — notifications Postgres connection string",
    ),
    (
        "CATALYRST_NOTIFICATIONS_ADMIN_TOKEN",
        "optional — bearer token guarding the admin endpoints",
    ),
    (
        "SENDGRID_API_KEY",
        "optional — enables outbound email via SendGrid",
    ),
    ("SENDGRID_FROM_EMAIL", "optional — SendGrid from address"),
    (
        "SENDGRID_VALIDATE_EMAIL_TEMPLATE_ID",
        "optional — SendGrid template for email validation",
    ),
    (
        "SENDGRID_VALIDATE_CREDITS_EMAIL_TEMPLATE_ID",
        "optional — SendGrid template for credits email validation",
    ),
    (
        "ACCOUNT_BASE_URL",
        "account site base URL (default https://account.decentraland.org)",
    ),
    (
        "MARKETPLACE_BASE_URL",
        "marketplace base URL (default https://decentraland.org/marketplace)",
    ),
    (
        "TURNSTILE_SECRET_KEY",
        "optional — Cloudflare Turnstile secret for email subscription captcha",
    ),
    (
        "EMAIL_DOMAIN_BLACKLIST",
        "optional — comma-separated email domains to reject",
    ),
    (
        "CONTENT_PG_CONNECTION_STRING",
        "optional — catalyst content DB connection string",
    ),
    (
        "SOCIAL_PG_CONNECTION_STRING",
        "optional — social DB connection string",
    ),
    (
        "SQUID_PG_CONNECTION_STRING",
        "optional — squid DB connection string",
    ),
    (
        "TELEMETRY_PG_CONNECTION_STRING",
        "optional — telemetry DB connection string",
    ),
    (
        "SHOP_ITEM_BASE_URL",
        "shop item link base (default https://decentraland.org/marketplace/)",
    ),
    (
        "RUST_LOG",
        "tracing filter (default catalyrst_notifications=info,tower_http=info)",
    ),
];

#[tokio::main]
async fn main() -> Result<()> {
    catalyrst_envcfg::handle_standard_args("catalyrst-notifications", ENV_DOCS);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "catalyrst_notifications=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    let state = build_state(&cfg).await?;

    let app = Router::new()
        .route("/ping", get(handlers::ping::ping))
        .route("/health", get(handlers::ping::ping))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.http_host, cfg.http_port).parse()?;
    tracing::info!(%addr, "catalyrst-notifications listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
