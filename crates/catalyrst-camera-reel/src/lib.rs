pub mod admin;
pub mod auth_chain;
pub mod config;
pub mod docs;
pub mod dto;
pub mod handlers;
pub mod http;
pub mod ports;

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::ports::db::Database;
use crate::ports::places::PlacesClient;
use crate::ports::storage::ImageStore;

pub struct AppStateInner {
    pub config: Config,
    pub db: Database,
    pub store: ImageStore,
    pub places: PlacesClient,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: Config) -> Result<AppState> {
    let opts = PgConnectOptions::from_str(&cfg.database_url)
        .context("invalid CAMERA_REEL_PG_CONNECTION_STRING")?
        .options([
            ("statement_timeout", "60000"),
            ("idle_in_transaction_session_timeout", "30000"),
        ]);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .idle_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .context("failed to connect places_events pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to run camera-reel migrations")?;

    let store = ImageStore::new(&cfg.content_storage_dir)
        .await
        .context("failed to initialize content storage")?;

    let places = PlacesClient::new(
        cfg.places_api_url.clone(),
        cfg.places_cache_ttl_seconds,
        cfg.places_cache_max_size,
    );

    Ok(Arc::new(AppStateInner {
        db: Database::new(pool),
        store,
        places,
        config: cfg,
    }))
}

pub fn api_router() -> Router<AppState> {
    let api = Router::new()
        .route("/images", post(handlers::images::upload_image))
        .route("/images/{image_id}", get(handlers::images::get_image))
        .route("/images/{image_id}", delete(handlers::images::delete_image))
        .route(
            "/images/{image_id}/visibility",
            patch(handlers::images::update_image_visibility),
        )
        .route(
            "/images/{image_id}/metadata",
            get(handlers::images::get_metadata),
        )
        .route("/users/{user_address}", get(handlers::users::get_user_data))
        .route(
            "/users/{user_address}/images",
            get(handlers::users::get_user_images),
        )
        .route(
            "/places/{place_id}/images",
            get(handlers::places::get_place_images),
        )
        .route(
            "/places/images",
            post(handlers::places::get_multiple_places_images),
        )
        .route("/docs/openapi.json", get(docs::openapi_json))
        .route("/docs/ui", get(docs::swagger_ui))
        .route("/docs/ui/", get(docs::swagger_ui))
        .route("/docs/ui/{*rest}", get(docs::swagger_ui));

    // Moderator admin routes. Bearer-gated (CATALYRST_CAMERA_REEL_ADMIN_TOKEN),
    // mounted at top level to match the admin-console spec paths exactly.
    let admin = Router::new()
        .route(
            "/admin/images/{image_id}",
            delete(handlers::images::admin_delete_image),
        )
        .route(
            "/admin/images/{image_id}/review",
            patch(handlers::images::admin_update_image_review),
        );

    Router::new().nest("/api", api).merge(admin)
}
