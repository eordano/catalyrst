pub mod auth_chain;
pub mod captcha;
pub mod config;
pub mod dto;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod provider;
pub mod purchase_intent;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::ports::checkout::OutboxWorker;
use crate::ports::credits::CreditsComponent;
use crate::ports::escrow::ReleaseWorker;
use crate::ports::pricing::PricingClient;
use crate::ports::stripe::{StripeClient, DEFAULT_STRIPE_API_BASE};
use crate::provider::CaptchaProvider;

pub struct AppStateInner {
    pub credits: CreditsComponent,

    pub admin_token: Option<String>,

    pub captcha_provider: Option<CaptchaProvider>,

    pub stripe: Option<StripeClient>,

    pub stripe_webhook_secret: Option<String>,

    pub mock_card: bool,

    pub credits_currency: String,

    pub pricing: PricingClient,

    pub checkout_fulfillment_mode: String,

    pub require_purchase_intent: bool,

    pub economy_base_url: String,

    pub economy_admin_token: Option<String>,

    pub escrow_address: Option<String>,

    pub usage_grants_pool: Option<sqlx::PgPool>,

    pub economy_http: reqwest::Client,

    pub quote_cache: handlers::prices::QuoteCache,
}

pub type AppState = Arc<AppStateInner>;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/users", post(handlers::users::enroll))
        .route(
            "/users/{wallet_id}/progress",
            get(handlers::users::progress),
        )
        .route(
            "/wallet/{wallet_id}/balance",
            get(handlers::wallet::balance),
        )
        .route("/packs", get(handlers::packs::list_packs))
        .route("/packs/{sku}/intent", post(handlers::packs::create_intent))
        .route(
            "/packs/{sku}/mock-purchase",
            post(handlers::packs::mock_purchase),
        )
        .route("/topup/mock-card", post(handlers::packs::mock_topup))
        .route("/topup/mana", post(handlers::topup::mana_topup))
        .route("/topup/mana/quote", get(handlers::topup::mana_topup_quote))
        .route("/stripe/webhook", post(handlers::stripe::webhook))
        .route("/cart", get(handlers::cart::get_cart))
        .route("/cart/items", post(handlers::cart::add_item))
        .route(
            "/cart/items/{collection}/{item_id}",
            axum::routing::delete(handlers::cart::remove_item),
        )
        .route("/checkout", post(handlers::cart::checkout))
        .route("/checkout/{id}", get(handlers::cart::get_checkout))
        .route("/prices/quote", post(handlers::prices::quote))
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

    let stripe = cfg.stripe_secret_key.clone().map(|secret| {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build reqwest client");
        let base_url = std::env::var("STRIPE_API_BASE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_STRIPE_API_BASE.to_string());
        StripeClient::new(secret, base_url, client)
    });

    let credits = CreditsComponent::new(pool);

    let pricing_http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("failed to build reqwest client");
    let pricing = PricingClient::new(
        pricing_http,
        cfg.market_base_url.clone(),
        cfg.price_base_url.clone(),
        cfg.marketplace_markup_bps,
        cfg.mana_price_max_staleness_secs,
    );

    let usage_grants_pool = match &cfg.usage_grants_database_url {
        Some(url) => match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect(url)
            .await
        {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::error!(error = %e, "failed to connect usage_grants pool; grant writes disabled");
                None
            }
        },
        None => None,
    };

    if stripe.is_none() {
        tracing::warn!(
            "STRIPE_SECRET_KEY unset: card-purchase routes are DISABLED \
             (POST /packs/{{sku}}/intent and POST /stripe/webhook return 501). \
             Set STRIPE_SECRET_KEY + STRIPE_WEBHOOK_SECRET to enable pack purchases."
        );
    }
    if usage_grants_pool.is_none() {
        tracing::error!(
            "USAGE_GRANTS_PG_CONNECTION_STRING unset (or unreachable): the escrow/lease overlay \
             is OFF, so POST /checkout is DISABLED (refusing to debit Credits for items that \
             would be invisible in the backpack). Set USAGE_GRANTS_PG_CONNECTION_STRING to enable \
             checkout."
        );
    }

    let worker_http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client");
    OutboxWorker {
        credits: credits.clone(),
        pricing: pricing.clone(),
        http: worker_http.clone(),
        economy_base_url: cfg.economy_base_url.clone(),
        economy_admin_token: cfg.economy_admin_token.clone(),
        escrow_address: cfg.landiler_escrow_address.clone(),
        max_attempts: cfg.checkout_max_attempts,
        usage_grants_pool: usage_grants_pool.clone(),
        escrow_lock_days: cfg.escrow_lock_days,
        mock_fulfillment: cfg.mock_fulfillment,
    }
    .spawn(cfg.checkout_worker_interval_secs);

    ReleaseWorker {
        http: worker_http.clone(),
        economy_base_url: cfg.economy_base_url.clone(),
        economy_admin_token: cfg.economy_admin_token.clone(),
        escrow_address: cfg.landiler_escrow_address.clone(),
        usage_grants_pool: usage_grants_pool.clone(),
    }
    .spawn(cfg.checkout_worker_interval_secs);

    let progress_presence_pool = match &cfg.progress_presence_database_url {
        Some(url) => match PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(60)))
            .connect(url)
            .await
        {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "failed to connect presence pool; explorer goal tracking disabled"
                );
                None
            }
        },
        None => None,
    };
    ports::progress::spawn_progress_worker(
        credits.clone(),
        progress_presence_pool,
        cfg.checkout_worker_interval_secs,
    );

    Ok(Arc::new(AppStateInner {
        credits,
        admin_token: cfg.admin_token.clone(),
        captcha_provider,
        stripe,
        stripe_webhook_secret: cfg.stripe_webhook_secret.clone(),
        mock_card: cfg.mock_card,
        credits_currency: cfg.credits_currency.clone(),
        pricing,
        checkout_fulfillment_mode: cfg.checkout_fulfillment_mode.clone(),
        require_purchase_intent: cfg.require_purchase_intent,
        economy_base_url: cfg.economy_base_url.clone(),
        economy_admin_token: cfg.economy_admin_token.clone(),
        escrow_address: cfg.landiler_escrow_address.clone(),
        usage_grants_pool,
        economy_http: worker_http,
        quote_cache: handlers::prices::QuoteCache::default(),
    }))
}
