pub mod auth_chain;
pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, patch, post};
use axum::Router;
use reqwest::Client;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::ports::items::{ItemsComponent, NewsletterComponent};

pub struct AppStateInner {
    pub items: ItemsComponent,
    pub newsletter: NewsletterComponent,
    pub content_bucket_url: String,
    pub admin_addresses: Vec<String>,
    pub newsletter_service_url: Option<String>,
    pub newsletter_publication_id: Option<String>,
    pub newsletter_api_key: Option<String>,
    pub admin_token: Option<String>,
    pub http: Client,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(60)))
        .connect(&cfg.database_url)
        .await
        .context("failed to connect to builder database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("builder migrations failed")?;

    Ok(Arc::new(AppStateInner {
        items: ItemsComponent::new(pool.clone()),
        newsletter: NewsletterComponent::new(pool.clone()),
        content_bucket_url: cfg.content_bucket_url.clone(),
        admin_addresses: cfg.admin_addresses.clone(),
        newsletter_service_url: cfg.newsletter_service_url.clone(),
        newsletter_publication_id: cfg.newsletter_publication_id.clone(),
        newsletter_api_key: cfg.newsletter_api_key.clone(),
        admin_token: cfg.admin_token.clone(),
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build http client")?,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/collections/{id}/items",
            get(handlers::collections::get_collection_items),
        )
        .route(
            "/v1/storage/contents/{hash}",
            get(handlers::storage::get_storage_content)
                .head(handlers::storage::get_storage_content),
        )
        .route(
            "/v1/storage/contents/{hash}/exists",
            get(handlers::storage::head_storage_content_exists),
        )
        .route("/v1/newsletter", post(handlers::newsletter::post_newsletter))
        .route(
            "/v1/collections/{id}/items/{item}/status",
            patch(handlers::curation::patch_item_status),
        )
        .route(
            "/v1/collections/{id}/items/status",
            patch(handlers::curation::patch_items_status_bulk),
        )
}
