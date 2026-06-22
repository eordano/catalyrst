pub mod auth_chain;
pub mod captcha;
pub mod config;
pub mod dto;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod provider;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::ports::credits::CreditsComponent;
use crate::provider::CaptchaProvider;

pub struct AppStateInner {
    pub credits: CreditsComponent,
    /// Bearer token gating the admin routes; `None` => fail closed (403).
    pub admin_token: Option<String>,
    /// External captcha provider; `None` => the upstream slider gate stands alone.
    pub captcha_provider: Option<CaptchaProvider>,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/users", post(handlers::users::enroll))
        .route(
            "/users/{wallet_id}/progress",
            get(handlers::users::progress),
        )
        .route("/seasons", get(handlers::seasons::seasons))
        .route(
            "/captcha",
            get(handlers::captcha::generate).post(handlers::captcha::claim),
        )
        .merge(handlers::admin::router())
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
}

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect to credits database")?;

    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        tracing::error!(error = %e, "migration failed");
        return Err(e.into());
    }

    let captcha_provider = cfg.captcha_secret.clone().map(|secret| {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        CaptchaProvider::new(secret, cfg.captcha_verify_url.clone(), client)
    });

    Ok(Arc::new(AppStateInner {
        credits: CreditsComponent::new(pool),
        admin_token: cfg.admin_token.clone(),
        captcha_provider,
    }))
}
