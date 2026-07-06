pub mod config;
pub mod handlers;
pub mod http;
pub mod ports;
pub mod resolve;

pub use dcl_contents::{content, manifest_store, types};

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{delete, get, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use dcl_contents::content::ContentComponent;
use dcl_contents::manifest_store::AbManifestStore;
use dcl_contents::registry::{RegistryAppState, RegistryStateInner};

use crate::config::Config;
use crate::ports::registry::RegistryStore;

pub struct AppStateInner {
    pub content: ContentComponent,
    pub manifests: AbManifestStore,
    pub registry: RegistryStore,
    pub admin_token: Option<String>,
    pub profile_images_url: String,
    pub denylist_moderators: Vec<String>,
    pub contents_state: RegistryAppState,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let content_opts = PgConnectOptions::from_str(&cfg.content_database_url)
        .context("invalid content DB connection string")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let content_pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(content_opts)
        .await
        .context("failed to connect content DB pool")?;

    let registry_pool = match &cfg.ab_registry_database_url {
        Some(url) => {
            let opts = PgConnectOptions::from_str(url)
                .context("invalid ab_registry DB connection string")?;
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .connect_with(opts)
                .await
                .context("failed to connect ab_registry DB pool")?;
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .context("ab_registry migrations failed")?;
            Some(pool)
        }
        None => {
            tracing::warn!(
                "AB_REGISTRY_PG_CONNECTION_STRING unset — denylist + spawn overrides disabled"
            );
            None
        }
    };

    if ["127.0.0.1", "localhost", "[::1]", "0.0.0.0"]
        .iter()
        .any(|lo| cfg.profile_images_url.contains(lo))
    {
        tracing::warn!(
            base = %cfg.profile_images_url,
            "profile snapshot URLs use a LOOPBACK base — clients cannot fetch them; \
             set PROFILE_CDN_BASE_URL to the public gateway base"
        );
    }

    let content = ContentComponent::new(content_pool);
    let manifests = AbManifestStore::new(cfg.abgen_out_root.clone());
    let registry = RegistryStore::new(registry_pool);
    let contents_state = Arc::new(RegistryStateInner {
        content: Arc::new(content.clone()),
        manifests: manifests.clone(),
        profile_images_url: cfg.profile_images_url.clone(),
        world_policy: Arc::new(registry.clone()),
    });

    Ok(Arc::new(AppStateInner {
        content,
        manifests,
        registry,
        admin_token: cfg.admin_token.clone(),
        profile_images_url: cfg.profile_images_url.clone(),
        denylist_moderators: cfg.denylist_moderators.clone(),
        contents_state,
    }))
}

pub fn api_router() -> Router<AppState> {
    index_router()
        .merge(contents_router())
        .merge(signed_router())
}

fn index_router() -> Router<AppState> {
    Router::new()
        .route(
            "/entities/active",
            post(handlers::entities::post_entities_active),
        )
        .route(
            "/entities/versions",
            post(handlers::entities::post_entities_versions),
        )
}

fn contents_router() -> Router<AppState> {
    Router::new()
        .route("/profiles", post(handlers::contents::post_profiles))
        .route(
            "/profiles/metadata",
            post(handlers::contents::post_profiles_metadata),
        )
        .route(
            "/entities/status/{id}",
            get(handlers::contents::get_entity_status),
        )
        .route(
            "/worlds/{world_name}/manifest",
            get(handlers::contents::get_world_manifest),
        )
}

pub fn signed_router() -> Router<AppState> {
    Router::new()
        .route(
            "/entities/status",
            get(handlers::status::get_entities_status_signed),
        )
        .route("/queues/status", get(handlers::queues::get_queues_status))
        .route("/queues/retry", post(handlers::queues::post_queues_retry))
        .route("/queues/pause", post(handlers::queues::post_queues_pause))
        .route("/queues/resume", post(handlers::queues::post_queues_resume))
        .route("/denylist", get(handlers::denylist::get_denylist))
        .route(
            "/denylist/{entity_id}",
            post(handlers::denylist::add_denylist).delete(handlers::denylist::remove_denylist),
        )
        .route("/registry", post(handlers::admin::post_registry))
        .route("/flush-cache", delete(handlers::admin::flush_cache))
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_router_constructs_without_route_conflicts() {
        let _ = super::api_router();
    }
}
