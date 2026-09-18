pub mod auth_chain;
pub mod catalog_build;
pub mod catalog_store;
pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod pull_cache;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{get, patch, post};
use axum::Router;
use reqwest::Client;

use crate::catalog_store::{CatalogStore, PullThrough};
use crate::config::Config;
use crate::ports::items::{ItemsComponent, NewsletterComponent};
use crate::ports::marketplace::MarketplaceComponent;

pub const MARKETPLACE_SQUID_SCHEMA: &str = "squid_marketplace";

pub struct AppStateInner {
    pub items: ItemsComponent,
    pub newsletter: NewsletterComponent,

    pub marketplace: Option<MarketplaceComponent>,
    pub content_bucket_url: String,
    pub polygon_rpc_url: Option<String>,
    pub catalog: Option<Arc<CatalogStore>>,
    pub admin_addresses: Vec<String>,
    pub newsletter_service_url: Option<String>,
    pub newsletter_publication_id: Option<String>,
    pub newsletter_api_key: Option<String>,
    pub admin_token: Option<String>,
    pub http: Client,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = catalyrst_db::connect_pool(
        &cfg.database_url,
        &catalyrst_db::PoolSettings {
            idle_timeout_secs: 60,
            acquire_timeout_secs: Some(10),
            ..catalyrst_db::PoolSettings::default()
        },
    )
    .await
    .context("failed to connect to builder database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("builder migrations failed")?;

    let marketplace = match &cfg.marketplace_database_url {
        Some(url) => {
            let mp_pool = catalyrst_db::connect_pool(url, &catalyrst_db::PoolSettings::side_pool())
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

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build http client")?;
    let catalog = match &cfg.catalog_dir {
        Some(dir) => {
            let pull = if cfg.catalog_pull_cache_bytes > 0 {
                tracing::info!(
                    budget_bytes = cfg.catalog_pull_cache_bytes,
                    bucket = %cfg.content_bucket_url,
                    "builder content pull-through enabled"
                );
                Some(PullThrough {
                    bucket: cfg.content_bucket_url.clone(),
                    budget: cfg.catalog_pull_cache_bytes,
                    http: reqwest::Client::builder()
                        .timeout(Duration::from_secs(120))
                        .build()
                        .context("failed to build content http client")?,
                })
            } else {
                tracing::info!(
                    "BUILDER_CATALOG_PULL_CACHE_BYTES unset; /contents/{{hash}} serves only the local store"
                );
                None
            };
            Some(Arc::new(
                CatalogStore::new(dir.clone(), pull)
                    .with_context(|| format!("open builder catalog store {}", dir.display()))?,
            ))
        }
        None => {
            tracing::warn!(
                "BUILDER_CATALOG_DIR unset; /v1/assetPacks and /contents/{{hash}} return 503"
            );
            None
        }
    };

    Ok(Arc::new(AppStateInner {
        items: ItemsComponent::new(pool.clone()),
        newsletter: NewsletterComponent::new(pool.clone()),
        marketplace,
        content_bucket_url: cfg.content_bucket_url.clone(),
        polygon_rpc_url: cfg.polygon_rpc_url.clone(),
        catalog,
        admin_addresses: cfg.admin_addresses.clone(),
        newsletter_service_url: cfg.newsletter_service_url.clone(),
        newsletter_publication_id: cfg.newsletter_publication_id.clone(),
        newsletter_api_key: cfg.newsletter_api_key.clone(),
        admin_token: cfg.admin_token.clone(),
        http,
    }))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/items/{id}/files",
            post(handlers::drafts::post_item_files)
                .layer(axum::extract::DefaultBodyLimit::max(21 * 1024 * 1024)),
        )
        .route(
            "/v1/collections/{id}/publication",
            get(handlers::drafts::get_publication)
                .post(handlers::publication::begin)
                .put(handlers::publication::transaction)
                .delete(handlers::publication::cancel)
                .patch(handlers::publication::claim),
        )
        .route(
            "/v1/collections/{id}/publication/status",
            get(handlers::publication::status),
        )
        .route(
            "/v1/collections/{id}/linked-publication",
            get(handlers::linked_publication::prepare)
                .post(handlers::linked_publication::begin)
                .patch(handlers::linked_publication::claim)
                .put(handlers::linked_publication::authorize)
                .delete(handlers::linked_publication::cancel)
                .layer(axum::middleware::map_response(
                    handlers::linked_publication::private_response,
                )),
        )
        .route(
            "/v1/collections/{id}/linked-publication/status",
            get(handlers::linked_publication::status).layer(axum::middleware::map_response(
                handlers::linked_publication::private_response,
            )),
        )
        .route(
            "/v1/collections/{id}/linked-publication/verify",
            post(handlers::linked_publication::verify).layer(axum::middleware::map_response(
                handlers::linked_publication::private_response,
            )),
        )
        .route("/v1/collections", get(handlers::drafts::get_drafts))
        .route("/v1/items", get(handlers::drafts::get_item_drafts))
        .route(
            "/v1/items/{id}",
            get(handlers::drafts::get_item).put(handlers::drafts::put_item),
        )
        .route(
            "/v1/collections/{id}/items",
            get(handlers::collections::get_collection_items),
        )
        .route(
            "/v1/collections/{id}",
            get(handlers::collections::get_collection).put(handlers::drafts::put_collection),
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
        .route("/v1/assetPacks", get(handlers::catalog::get_asset_packs))
        .route("/contents/{hash}", get(handlers::catalog::get_content))
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
