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
use crate::ports::marketplace::MarketplaceComponent;

pub const MARKETPLACE_SQUID_SCHEMA: &str = "squid_marketplace";

pub struct AppStateInner {
    pub items: ItemsComponent,
    pub newsletter: NewsletterComponent,

    pub marketplace: Option<MarketplaceComponent>,
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

    let marketplace = match &cfg.marketplace_database_url {
        Some(url) => {
            let mp_pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .idle_timeout(Some(Duration::from_secs(60)))
                .connect(url)
                .await
                .context("failed to connect to marketplace squid database")?;
            Some(MarketplaceComponent::new(mp_pool))
        }
        None => {
            tracing::warn!(
                "BUILDER_MARKETPLACE_PG_CONNECTION_STRING unset; \
                 /v1/{{address}}/collections and /v1/{{address}}/items return 503"
            );
            None
        }
    };

    Ok(Arc::new(AppStateInner {
        items: ItemsComponent::new(pool.clone()),
        newsletter: NewsletterComponent::new(pool.clone()),
        marketplace,
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
            "/v1/collections/{id}",
            get(handlers::collections::get_collection),
        )
        .route(
            "/v1/collections/curation",
            get(handlers::curation::get_curation_collections),
        )
        .route(
            "/v1/{address}/collections",
            get(handlers::onchain::get_address_collections),
        )
        .route(
            "/v1/{address}/items",
            get(handlers::onchain::get_address_items),
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
        .route(
            "/v1/newsletter",
            post(handlers::newsletter::post_newsletter),
        )
        .route(
            "/v1/collections/{id}/items/{item}/status",
            patch(handlers::curation::patch_item_status),
        )
        .route(
            "/v1/collections/{id}/items/status",
            patch(handlers::curation::patch_items_status_bulk),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_builds_without_route_conflicts() {
        let _: Router<AppState> = api_router();
    }
}
