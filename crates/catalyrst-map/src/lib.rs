#![allow(clippy::result_large_err)]

pub mod cache;
pub mod config;
pub mod districts;
pub mod handlers;
pub mod map;
pub mod nft;
pub mod proximity;
pub mod render;
pub mod rentals;
pub mod satellite;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::routing::get;
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::map::MapComponent;
use crate::satellite::SatelliteState;

pub struct AppStateInner {
    pub map: MapComponent,
    pub pool: PgPool,
    pub map_schema: String,
    pub satellite: Arc<SatelliteState>,
}

pub type AppState = Arc<AppStateInner>;

pub async fn build_state(cfg: &Config) -> Result<AppState> {
    let pool = catalyrst_db::connect_pool(
        &cfg.database_url,
        &catalyrst_db::PoolSettings {
            statement_timeout_ms: 120_000,
            ..catalyrst_db::PoolSettings::default()
        },
    )
    .await
    .context("failed to connect marketplace_squid pool")?;

    let map = MapComponent::new(
        pool.clone(),
        cfg.schema.clone(),
        cfg.land_contract_address.clone(),
        cfg.estate_contract_address.clone(),
        cfg.map_tiles_cache_entries,
        cfg.map_png_cache_entries,
    );

    let satellite = SatelliteState::new(
        cfg.satellite_dir.clone(),
        cfg.satellite_source_budget_bytes,
        cfg.satellite_output_entries,
    );
    tracing::info!(
        dir = %cfg.satellite_dir.display(),
        "satellite tile renderer ready"
    );

    let state = Arc::new(AppStateInner {
        map: map.clone(),
        pool: pool.clone(),
        map_schema: cfg.schema.clone(),
        satellite: satellite.clone(),
    });

    tracing::info!("building initial tile grid...");
    match map.refresh().await {
        Ok(()) => tracing::info!(
            tiles = map.snapshot().map(|d| d.tiles.len()).unwrap_or(0),
            "tile grid ready"
        ),
        Err(e) => {
            tracing::error!(error = %e, "initial tile grid build failed; serving 503 until next refresh")
        }
    }

    {
        let map = map.clone();
        let interval = cfg.refresh_interval_secs;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(interval));
            tick.tick().await;
            loop {
                tick.tick().await;
                match map.refresh().await {
                    Ok(()) => tracing::debug!("tile grid refreshed"),
                    Err(e) => tracing::warn!(error = %e, "tile grid refresh failed"),
                }
            }
        });
    }

    {
        let satellite = satellite.clone();
        let interval = cfg.satellite_scan_secs.max(1);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(interval));
            tick.tick().await;
            loop {
                tick.tick().await;
                let s = satellite.clone();

                let _ = tokio::task::spawn_blocking(move || s.scan()).await;
            }
        });
    }

    Ok(state)
}

pub fn service_router(state: AppState) -> Router {
    Router::new()
        .route("/ping", get(handlers::status::ping))
        .route("/ready", get(handlers::status::ready))
        .route("/v2/ping", get(handlers::status::ping))
        .route("/v2/ready", get(handlers::status::ready))
        .merge(api_router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/v1/map.png", get(handlers::map_png::map_png))
        .route("/v2/map.png", get(handlers::map_png::map_png))
        .route("/v1/minimap.png", get(handlers::map_png::minimap_png))
        .route(
            "/v1/estatemap.png",
            get(handlers::map_png::estate_minimap_png),
        )
        .route(
            "/v1/parcels/{x}/{y}/map.png",
            get(handlers::map_png::parcel_map_png),
        )
        .route(
            "/v2/parcels/{x}/{y}/map.png",
            get(handlers::map_png::parcel_map_png),
        )
        .route(
            "/v1/estates/{estate_id}/map.png",
            get(handlers::map_png::estate_map_png),
        )
        .route(
            "/v2/estates/{estate_id}/map.png",
            get(handlers::map_png::estate_map_png),
        )
        .route("/v1/tiles", get(handlers::tiles::get_legacy_tiles))
        .route("/v2/tiles", get(handlers::tiles::get_tiles))
        .route("/v2/tiles/info", get(handlers::tiles::tiles_info))
        .route("/v2/parcels/{x}/{y}", get(handlers::meta::get_parcel))
        .route("/v2/estates/{id}", get(handlers::meta::get_estate))
        .route(
            "/v2/contracts/{address}/tokens/{id}",
            get(handlers::meta::get_token),
        )
        .route("/v2/districts", get(handlers::meta::get_districts))
        .route("/v2/districts/{id}", get(handlers::meta::get_district))
        .route(
            "/v2/addresses/{address}/contributions",
            get(handlers::meta::get_contributions),
        )
        .route(
            "/satellite/{z}/{x}/{y}",
            get(handlers::satellite::satellite_tile),
        )
}
